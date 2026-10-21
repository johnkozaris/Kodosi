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
    private(set) var revision = 0
    private(set) var bells: [String: Int] = [:]
    var watchingSessionId: String? {
        didSet {
            guard watchingSessionId != oldValue else { return }
            let visible = watchingSessionId.map { [JSONValue.string($0)] } ?? []
            commands.perform("session.visible", ["sessionIds": .array(visible)])
        }
    }

    @ObservationIgnored let commands: CommandSink
    @ObservationIgnored let notices: NoticeCenter
    @ObservationIgnored let focus: TerminalProcessFocusCoordinator
    @ObservationIgnored let live: TerminalSessionManager

    init(commands: CommandSink, notices: NoticeCenter, runtime: RuntimeHandle) {
        self.commands = commands
        self.notices = notices
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
        bells = [:]
        loaded = false
        watchingSessionId = nil
    }

    func releaseAll() {
        releaseLive()
        watchingSessionId = nil
    }

    func session(_ id: String) -> RuntimeSession? {
        sessions.first { $0.id == id }
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
        case "term.bell":
            guard let id = event.string("sessionId"), watchingSessionId == id else { return }
            bells[id, default: 0] += 1
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

    func refresh() async {
        let before = revision
        commands.perform("session.list")
        await RefreshWait.until { revision != before }
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

    func interrupt(_ session: RuntimeSession) {
        act("session.interrupt", on: session)
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
