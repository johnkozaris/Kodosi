import Foundation
import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions
import Observation

@Observable
final class TerminalStore {
    private(set) var sessions: [RuntimeSession] = []
    private(set) var userId: String?
    private(set) var loaded = false
    private(set) var listed = false
    /// True once `session.list` has replied since returning from background, meaning `attention`
    /// flags are current and safe to settle push notifications against.
    @ObservationIgnored private(set) var fresh = false
    @ObservationIgnored private var listing: String?
    private(set) var revision = 0
    private(set) var faces: [String: FacePick] = [:]
    var watchingSessionId: String? {
        didSet {
            guard watchingSessionId != oldValue else { return }
            let visible = watchingSessionId.map { [JSONValue.string($0)] } ?? []
            commands.perform("session.visible", ["sessionIds": .array(visible), "watching": .bool(watchingSessionId != nil)])
        }
    }

    @ObservationIgnored let commands: CommandSink
    @ObservationIgnored let notices: NoticeCenter
    let canvas: CanvasStore
    @ObservationIgnored let documentPages = CanvasDocumentPages()
    @ObservationIgnored let focus: TerminalProcessFocusCoordinator
    @ObservationIgnored let live: TerminalSessionManager

    init(commands: CommandSink, notices: NoticeCenter, runtime: RuntimeHandle) {
        self.commands = commands
        self.notices = notices
        canvas = CanvasStore(commands: commands, videoPipes: runtime.videoPipes)
        let focus = TerminalProcessFocusCoordinator(commands: commands)
        self.focus = focus
        live = TerminalSessionManager(
            runtimeHandle: runtime,
            commandSink: commands,
            style: { TerminalAppearance.style(look: $0) },
            onFocusResult: { sessionId, incarnation, requestId, accepted in
                focus.applyResult(sessionId: sessionId, runtimeIncarnationId: incarnation, requestId: requestId, accepted: accepted)
            },
        )
    }

    var needsYou: [RuntimeSession] {
        TerminalCatalog.needsYou(sessions)
    }

    func reset(userId: String?) {
        releaseLive()
        self.userId = userId
        sessions = []
        canvas.observe([])
        loaded = false
        listed = false
        fresh = false
        listing = nil
        watchingSessionId = nil
        faces = [:]
    }

    func stale() {
        fresh = false
    }

    func releaseAll() {
        releaseLive()
        watchingSessionId = nil
    }

    func session(_ id: String) -> RuntimeSession? {
        sessions.first { $0.id == id }
    }

    func replacement(of session: RuntimeSession) -> RuntimeSession? {
        guard let owner = session.ownerUserId, let host = session.hostDeviceId else { return nil }
        return sessions.restart(of: session.id, owner: owner, host: host)
    }

    func face(of session: RuntimeSession) -> TerminalFace {
        session.shownFace(faces[session.id])
    }

    func pick(_ face: TerminalFace, of id: String, over own: TerminalFace?) {
        faces[id] = FacePick(face, over: own)
    }

    func sign(of session: RuntimeSession) -> StatusSign.Form? {
        session.sign(unseen: session.attention)
    }

    func watch(_ sessionId: String, _ watching: Bool) {
        if watching {
            if watchingSessionId != sessionId {
                watchingSessionId = sessionId
            }
        } else if watchingSessionId == sessionId {
            watchingSessionId = nil
        }
    }

    func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "sessions.snapshot":
            try receiveSnapshot(event)
        case "session.result":
            settleList(event, fresh: true)
        case "document.opened", "document.lost", "document.frame", "document.closed":
            canvas.receive(event)
        case "session.picture":
            live.receivePicture(event)
        default:
            break
        }
    }

    private func receiveSnapshot(_ event: RuntimeEvent) throws {
        let received: [RuntimeSession] = try event.value("sessions")
        guard Set(received.map(\.id)).count == received.count else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got a terminal list it cannot read."))
        }
        revision += 1
        guard !loaded || received != sessions else { return }
        let before = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        let ids = Set(received.map(\.id))
        sessions = received
        canvas.observe(received)
        faces = faces.filter { id, pick in received.first { $0.id == id }?.keeps(pick) == true }
        loaded = true
        for value in received {
            if let old = before[value.id], old.incarnationId != value.incarnationId {
                focus.releaseSession(sessionId: value.id)
            }
        }
        for id in Set(before.keys).subtracting(ids) {
            focus.releaseSession(sessionId: id)
        }
        live.reconcile(with: received)
    }

    /// Tracks the in-flight `session.list` request so empty-state UI waits for a server reply.
    func list() {
        let request = UUIDv7.generate()
        listing = request
        commands.perform("session.list", ["requestId": .string(request)])
    }

    func absorb(error event: RuntimeEvent) {
        settleList(event, fresh: false)
    }

    private func settleList(_ event: RuntimeEvent, fresh: Bool) {
        guard let listing, event.string("requestId") == listing else { return }
        self.listing = nil
        listed = true
        self.fresh = self.fresh || fresh
    }

    func refresh() async {
        let before = revision
        commands.perform("session.list")
        await RefreshWait.until { revision != before }
    }

    func notificationTarget(_ id: String) async -> RuntimeSession? {
        if let known = sessions.notificationTarget(id) {
            return known
        }
        _ = try? await commands.request("session.list")
        return sessions.notificationTarget(id)
    }

    func start(_ terminal: TerminalStart) async throws -> String? {
        try await commands.request("session.create", terminal.fields).string("sessionId")
    }

    func open(_ sessionId: String) async throws {
        _ = try await commands.request("session.openRemote", ["sessionId": .string(sessionId)])
    }

    func disconnect(_ sessionId: String) {
        commands.perform("session.disconnect", ["sessionId": .string(sessionId)])
    }

    func mutate(_ operation: String, session target: RuntimeSession, fields: [String: JSONValue] = [:]) async throws {
        try await commands.mutate(operation, session: target, current: session(target.id), fields: fields)
    }

    func rename(_ session: RuntimeSession, to name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name), name != session.name else { return }
        act("session.rename", on: session, fields: ["name": .string(name)])
    }

    func close(_ session: RuntimeSession) {
        act("session.close", on: session)
    }

    func hide(_ session: RuntimeSession) {
        act("session.hide", on: session)
    }

    func interrupt(_ session: RuntimeSession) {
        act("session.interrupt", on: session)
    }

    /// Lets the terminal's agent go on with the task that a restart cut off.
    func continueTask(_ session: RuntimeSession) {
        act("session.continue", on: session)
    }

    /// Shows what the terminal's branch folder changed on its canvas.
    func showChanges(_ session: RuntimeSession) {
        act("session.changes", on: session)
    }

    func changeAgent(_ change: AgentSettings, in session: RuntimeSession) {
        act("session.agent", on: session, fields: ["change": change.json])
    }

    func leave(_ session: RuntimeSession) {
        act("session.leave", on: session)
    }

    func share(_ session: RuntimeSession, inProject projectId: String?) async throws {
        try await mutate("session.attachProject", session: session, fields: ["projectId": projectId.map(JSONValue.string) ?? .null])
    }

    /// `expected` is who had the terminal when the sheet opened, so that a change made elsewhere
    /// since then is refused instead of undone.
    func share(_ session: RuntimeSession, with people: Set<String>, expected: Set<String>) async throws {
        try await mutate("session.share", session: session, fields: [
            "userIds": .array(people.sorted().map(JSONValue.string)),
            "expectedUserIds": .array(expected.sorted().map(JSONValue.string)),
        ])
    }

    private func act(_ operation: String, on session: RuntimeSession, fields: [String: JSONValue] = [:]) {
        Task {
            do {
                try await mutate(operation, session: session, fields: fields)
            } catch {
                notices.show(error)
            }
        }
    }

    private func releaseLive() {
        for session in sessions {
            focus.releaseSession(sessionId: session.id)
        }
        live.reconcileSessions(liveSessionIds: [])
    }
}
