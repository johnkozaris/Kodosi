import Foundation
@testable import Kodosi
import KodosiBridge
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

    @Test func readyRuntimeRefreshesTheSignedInAccount() throws {
        try deliver("system.ready", ["protocolVersion": RuntimeHandle.expectedProtocolVersion])
        #expect(model.phase == .ready)
        #expect(sent.types == ["session.list", "auth.refresh", "friends.refresh", "devices.refresh", "mission.list"])
    }

    @Test func signInOpensOnlySecurePagesAndGivesTheAnswerBack() throws {
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

    @Test func untrustedDeviceAsksForTrustUntilEnrolled() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": false])
        #expect(model.account.needsOnboarding)
        #expect(model.account.stage == .trustingDevice(.choose))
        try deliver("devices.list", ["selfDeviceId": "device-1", "localDeviceEnrolled": true, "devices": []])
        #expect(model.account.isReady)
        #expect(model.account.stage == .signedIn)
    }

    @Test func newerAccountEpochClearsEarlierState() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "working")]])
        #expect(model.terminals.sessions.count == 1)
        try deliver("sessions.snapshot", userId: "user-2", epoch: 2, ["sessions": []])
        #expect(model.terminals.sessions.count == 1)
        try deliver("auth.ready", userId: "user-2", epoch: 2, ["userId": "user-2", "enrolled": true])
        #expect(model.terminals.sessions.isEmpty)
        #expect(model.account.userId == "user-2")
    }

    @Test func anErrorShowsOnceAndOnlyForARequestOfTheApp() async throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        let creating = Task { try await model.commands.request("mission.create", ["requestId": .string("create"), "name": .string("Launch")]) }
        for _ in 0 ..< 100 where !sent.types.contains("mission.create") {
            await Task.yield()
        }
        try deliver("mission.error", ["requestId": "create", "operation": "mission.create", "message": "Use another name."])
        await #expect(throws: RuntimeError.operation("Use another name.")) { try await creating.value }
        #expect(model.notices.current == nil)
        try deliver("session.error", ["requestId": "command-line", "operation": "session.close", "message": "Not found."])
        #expect(model.notices.current == nil)
        try deliver("friends.error", ["operation": "friends.remove", "message": "Try again."])
        #expect(model.notices.current?.message == "Try again.")
    }

    @Test func aFailedFriendRequestOfThisAppShowsItsError() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        #expect(model.people.add("kodosi:nobody:00ff"))
        let requestId = try #require(sent.last("friends.request.send")?["requestId"] as? String)
        try deliver("friends.error", ["requestId": requestId, "operation": "friends.request.send", "message": "No such person."])
        #expect(model.notices.current?.message == "No such person.")
        #expect(!model.people.sendingRequest)
    }

    @Test func aTerminalThatArrivesDoneKeepsItsSignUntilYouLook() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        var shared = session(id: "s2", state: "done")
        shared["isOwner"] = false
        shared["connectionState"] = "offline"
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "done"), shared]])
        #expect(model.terminals.attention == ["s1"])
        model.terminals.watchingSessionId = "s1"
        #expect(model.terminals.attention.isEmpty)
    }

    @Test func blockedTerminalNeedsAttentionUnlessWatched() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "working"), session(id: "s2", state: "working")]])
        model.terminals.watchingSessionId = "s2"
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "blocked"), session(id: "s2", state: "blocked")]])
        #expect(model.terminals.attention == ["s1"])
        #expect(model.terminals.needsYou.map(\.id) == ["s1", "s2"])
    }

    private func session(id: String, state: String) -> [String: Any] {
        [
            "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
            "programStatus": ["state": state], "name": id, "isOwner": true, "status": "running",
            "connectionState": "connected", "sharedWith": [],
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
