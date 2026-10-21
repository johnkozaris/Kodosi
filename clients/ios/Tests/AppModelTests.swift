import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiDesign
import KodosiKit
import Testing

@MainActor
struct AppModelTests {
    private let model = AppModel()
    private let sent = SentCommands()

    init() {
        let sent = sent
        model.commands.transport = { data in
            RuntimeContract.verify(data)
            sent.append(data)
            return Int32(KODOSI_FFI_OK)
        }
    }

    private func deliver(_ type: String, userId: String? = "user-1", epoch: UInt64 = 1, _ fields: [String: Any] = [:]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = userId ?? NSNull()
        payload["accountEpoch"] = epoch
        let data = try JSONSerialization.data(withJSONObject: payload)
        try model.receive(JSONDecoder().decode(RuntimeEvent.self, from: data))
    }

    @Test func `ready runtime refreshes the signed in account`() throws {
        try deliver("system.ready", ["protocolVersion": RuntimeHandle.expectedProtocolVersion])
        #expect(model.phase == .ready)
        #expect(sent.types == ["session.list", "auth.refresh", "friends.refresh", "devices.refresh", "mission.list"])
    }

    @Test func `sign in opens only secure pages and gives the answer back`() throws {
        try deliver("auth.required", userId: nil)
        model.account.signIn()
        #expect(sent.types.last == "auth.login.start")
        try deliver("auth.browser", userId: nil, ["url": "http://example.com"])
        #expect(model.account.stage == .awaitingBrowser(nil))
        #expect(model.account.signInPage == nil)
        let page = "https://auth.kodosi.com/realms/kodosi/protocol/openid-connect/auth?state=s"
        try deliver("auth.browser", userId: nil, ["url": page])
        #expect(model.account.signInPage?.absoluteString == page)
        try model.account.finishSignIn(with: #require(URL(string: "com.kodosi:/auth?state=s&code=c")))
        #expect(sent.types.last == "auth.login.finish")
        #expect(model.account.signInPage == nil)
    }

    @Test func `untrusted device asks for trust until enrolled`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": false])
        #expect(model.account.needsOnboarding)
        #expect(model.account.stage == .trustingDevice(.choose))
        try deliver("devices.list", ["selfDeviceId": "device-1", "localDeviceEnrolled": true, "devices": []])
        #expect(model.account.isReady)
        #expect(model.account.stage == .signedIn)
    }

    @Test func `newer account epoch clears earlier state`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "working")]])
        #expect(model.terminals.sessions.count == 1)
        try deliver("sessions.snapshot", userId: "user-2", epoch: 2, ["sessions": []])
        #expect(model.terminals.sessions.count == 1)
        try deliver("auth.ready", userId: "user-2", epoch: 2, ["userId": "user-2", "enrolled": true])
        #expect(model.terminals.sessions.isEmpty)
        #expect(model.account.userId == "user-2")
    }

    @Test func `an error goes to its request or shows once`() async throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        let creating = Task { try await model.commands.request("mission.create", ["requestId": .string("create"), "name": .string("Launch")]) }
        for _ in 0 ..< 100 where !sent.types.contains("mission.create") {
            await Task.yield()
        }
        try deliver("mission.error", ["requestId": "create", "operation": "mission.create", "message": "Use another name."])
        await #expect(throws: RuntimeError.operation("Use another name.")) { try await creating.value }
        #expect(model.notices.current == nil)
        try deliver("friends.error", ["operation": "friends.remove", "message": "Try again."])
        #expect(model.notices.current?.message == "Try again.")
    }

    @Test func `a failed friend request of this app shows its error`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        #expect(model.people.add("kodosi:nobody:00ff"))
        let requestId = try #require(sent.last("friends.request.send")?["requestId"] as? String)
        try deliver("friends.error", ["requestId": requestId, "operation": "friends.request.send", "message": "No such person."])
        #expect(model.notices.current?.message == "No such person.")
        #expect(!model.people.sendingRequest)
    }

    @Test func `a done terminal shows its sign until the host knows you looked`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        var unseen = session(id: "s1", state: "done")
        unseen["attention"] = true
        try deliver("sessions.snapshot", ["sessions": [unseen, session(id: "s2", state: "done")]])
        #expect(try model.terminals.sign(of: #require(model.terminals.session("s1"))) == .done)
        #expect(try model.terminals.sign(of: #require(model.terminals.session("s2"))) == nil)
        model.terminals.watchingSessionId = "s1"
        #expect(sent.last("session.visible")?["sessionIds"] as? [String] == ["s1"])
    }

    @Test func `blocked terminals need you and the watched one is reported`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "blocked"), session(id: "s2", state: "blocked")]])
        model.terminals.watchingSessionId = "s2"
        #expect(sent.last("session.visible")?["sessionIds"] as? [String] == ["s2"])
        #expect(model.terminals.needsYou.map(\.id) == ["s1", "s2"])
    }

    private func session(id: String, state: String) -> [String: Any] {
        [
            "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
            "programStatus": ["state": state], "name": id, "isOwner": true, "status": "running",
            "attention": false, "connectionState": "connected", "sharedWith": [],
        ]
    }
}

final class SentCommands: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [[String: Any]] = []

    func append(_ data: Data) {
        guard let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        lock.withLock { values.append(value) }
    }

    var types: [String] {
        lock.withLock { values.compactMap { $0["type"] as? String } }
    }

    func last(_ type: String) -> [String: Any]? {
        lock.withLock { values.last { $0["type"] as? String == type } }
    }
}
