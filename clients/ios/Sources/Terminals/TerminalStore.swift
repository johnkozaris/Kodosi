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
    @ObservationIgnored private var listing: String?
    private(set) var revision = 0
    private(set) var bells: [String: Int] = [:]
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
        canvas = CanvasStore(commands: commands)
        let focus = TerminalProcessFocusCoordinator(commands: commands)
        self.focus = focus
        live = TerminalSessionManager(
            runtimeHandle: runtime,
            commandSink: commands,
            style: { TerminalAppearance.style },
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
        bells = [:]
        loaded = false
        listed = false
        listing = nil
        watchingSessionId = nil
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
            settleList(event)
        case "term.bell":
            guard let id = event.string("sessionId"), watchingSessionId == id else { return }
            bells[id, default: 0] += 1
        case "document.frame", "document.closed":
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
        loaded = true
        bells = bells.filter { ids.contains($0.key) }
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

    /// Asks for the account's terminals. Until the answer comes, the list is not known to be
    /// empty, so the empty state waits for it.
    func list() {
        let request = UUIDv7.generate()
        listing = request
        commands.perform("session.list", ["requestId": .string(request)])
    }

    func absorb(error event: RuntimeEvent) {
        settleList(event)
    }

    private func settleList(_ event: RuntimeEvent) {
        guard let listing, event.string("requestId") == listing else { return }
        self.listing = nil
        listed = true
    }

    func refresh() async {
        let before = revision
        commands.perform("session.list")
        await RefreshWait.until { revision != before }
    }

    /// Asks a computer to start a terminal, and gives its identifier.
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

    /// Changes what the agent in front of `session` works with. The agent's host types the
    /// agent's own commands once it waits for its next prompt.
    func changeAgent(_ change: AgentSettings, in session: RuntimeSession) {
        act("session.agent", on: session, fields: ["change": change.json])
    }

    func leave(_ session: RuntimeSession) {
        act("session.leave", on: session)
    }

    func share(_ session: RuntimeSession, inRoom roomId: String?) async throws {
        try await mutate("session.attachMission", session: session, fields: ["missionId": roomId.map(JSONValue.string) ?? .null])
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
