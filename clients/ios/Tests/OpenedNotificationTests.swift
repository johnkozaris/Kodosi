import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

struct OpenedNotificationTests {
    private let model = AppModel()
    private let log = TerminalCommandLog()

    init() throws {
        let log = log
        model.commands.transport { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        log.clear()
    }

    @Test func `after a cold start a notification waits for the terminal list and follows a restart of its terminal`() async throws {
        let target = Task { await model.terminals.notificationTarget("s1") }
        let request = try await listRequest()
        try deliver("sessions.snapshot", ["sessions": [
            terminalEntry("someone", state: "blocked", owner: false, restoredFrom: "s1"),
            terminalEntry("s2", state: "blocked", restoredFrom: "s1"),
        ]])
        try deliver("session.result", ["requestId": request, "operation": "session.list"])
        #expect(await target.value?.id == "s2")
    }

    private func listRequest() async throws -> String {
        await log.waitFor("session.list")
        return try #require(log.fields(of: "session.list").last?["requestId"] as? String)
    }

    private func deliver(_ type: String, _ fields: [String: Any] = [:]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        try model.receive(JSONDecoder().decode(RuntimeEvent.self, from: data))
    }
}
