import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct TerminalStatusLinkTests {
    private let model = AppModel()
    private let log = TerminalCommandLog()

    init() throws {
        let log = log
        model.commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        log.clear()
    }

    private func deliver(_ type: String, _ fields: [String: Any]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        try model.receive(JSONDecoder().decode(RuntimeEvent.self, from: data))
    }

    private func opened() -> [String?] {
        log.fields(of: "session.openRemote").map { $0["sessionId"] as? String }
    }

    @Test func ownTerminalsStayLinkedWhileTheAppIsInFront() throws {
        try deliver("sessions.snapshot", ["sessions": [
            terminalEntry("mine", state: "idle", connection: "offline"),
            terminalEntry("theirs", state: "idle", owner: false, connection: "offline"),
        ]])
        #expect(opened().isEmpty)
        model.terminals.keepStatusLinks(true)
        #expect(opened() == ["mine"])
        #expect(log.fields(of: "session.openRemote").allSatisfy { ($0["requestId"] as? String)?.isEmpty == false })
        model.terminals.disconnect("mine")
        #expect(log.fields(of: "session.disconnect").isEmpty)
        model.terminals.keepStatusLinks(false)
        #expect(log.fields(of: "session.disconnect").map { $0["sessionId"] as? String } == ["mine"])
    }

    @Test func terminalsThatCannotAnswerAreNotLinked() throws {
        var away = terminalEntry("away", state: "idle", connection: "offline")
        away["status"] = "reconnecting"
        var refused = terminalEntry("refused", state: "idle", connection: "offline")
        refused["message"] = "No access"
        try deliver("sessions.snapshot", ["sessions": [
            away, refused,
            terminalEntry("blocked", state: "idle", connection: "blocked"),
            terminalEntry("open", state: "idle", connection: "connected"),
        ]])
        model.terminals.keepStatusLinks(true)
        #expect(opened().isEmpty)
    }

    @Test func newTerminalsJoinUntilTheLimit() throws {
        model.terminals.keepStatusLinks(true)
        let many = (0 ..< 14).map { terminalEntry("t\($0)", state: "idle", connection: "offline") }
        try deliver("sessions.snapshot", ["sessions": many])
        #expect(opened().count == 12)
        try deliver("sessions.snapshot", ["sessions": many])
        #expect(opened().count == 12)
    }
}
