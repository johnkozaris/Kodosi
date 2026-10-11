import Foundation
import KodosiBridge
import KodosiDesign
import Observation

@Observable
final class TerminalStore {
    private(set) var sessions: [RuntimeSession] = []
    private(set) var attention: Set<String> = []
    private(set) var userId: String?
    var watchingSessionId: String? {
        didSet {
            if let watchingSessionId {
                attention.remove(watchingSessionId)
            }
        }
    }

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private let notices: NoticeCenter

    init(commands: CommandSink, notices: NoticeCenter) {
        self.commands = commands
        self.notices = notices
    }

    var needsYou: [RuntimeSession] {
        sessions.filter(\.needsUser)
    }

    func reset(userId: String?) {
        self.userId = userId
        sessions = []
        attention = []
        watchingSessionId = nil
    }

    func releaseAll() {
        watchingSessionId = nil
    }

    func session(_ id: String) -> RuntimeSession? {
        sessions.first { $0.id == id }
    }

    func sign(of session: RuntimeSession) -> StatusSign.Form? {
        session.sign(unseen: attention.contains(session.id))
    }

    func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "sessions.snapshot":
            try receiveSnapshot(event)
        case "term.bell":
            if let id = event.string("sessionId") {
                noteAttention(id)
            }
        default:
            break
        }
    }

    private func receiveSnapshot(_ event: RuntimeEvent) throws {
        let received: [RuntimeSession] = try event.value("sessions")
        guard Set(received.map(\.id)).count == received.count else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got a terminal list it cannot read."))
        }
        let before = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        sessions = received
        attention.formIntersection(received.map(\.id))
        for value in received {
            guard let old = before[value.id] else { continue }
            let finished = old.isWorking && !value.isWorking
            let waits = value.waitState != nil && value.waitState != old.waitState
            if finished || waits {
                noteAttention(value.id)
            }
        }
    }

    func noteAttention(_ id: String) {
        guard session(id) != nil, watchingSessionId != id else { return }
        attention.insert(id)
    }

    func seen(_ id: String) {
        attention.remove(id)
    }

    func mutate(_ operation: String, session target: RuntimeSession, fields: [String: JSONValue] = [:]) async throws {
        guard let current = session(target.id), current.incarnationId == target.incarnationId else {
            throw RuntimeError.operation(String(localized: "This terminal changed. Try again."))
        }
        var fields = fields
        fields["sessionId"] = .string(current.id)
        fields["expectedRuntimeIncarnationId"] = .string(current.incarnationId)
        _ = try await commands.request(operation, fields)
    }

    func rename(_ session: RuntimeSession, to name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name), name != session.name else { return }
        Task {
            do {
                try await mutate("session.rename", session: session, fields: ["name": .string(name)])
            } catch {
                notices.show(error)
            }
        }
    }

    func close(_ session: RuntimeSession) {
        Task {
            do {
                try await mutate("session.close", session: session)
            } catch {
                notices.show(error)
            }
        }
    }
}
