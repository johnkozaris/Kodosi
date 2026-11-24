import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiDesign
import KodosiKit
import KodosiTerminalSessions
import SwiftUI
import Testing

struct AppModelTests {
    private let model = AppModel()
    private let sent = SentCommands()

    init() {
        let sent = sent
        model.commands.transport { data in
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

    @Test func `newer account epoch clears earlier state`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1")]])
        #expect(model.terminals.sessions.count == 1)
        try deliver("sessions.snapshot", userId: "user-2", epoch: 2, ["sessions": []])
        #expect(model.terminals.sessions.count == 1)
        try deliver("auth.ready", userId: "user-2", epoch: 2, ["userId": "user-2", "enrolled": true])
        #expect(model.terminals.sessions.isEmpty)
        #expect(model.account.userId == "user-2")
    }

    @Test func `the phone is in Kodosi while it is on screen and away in the background, once for each runtime connection`() throws {
        let ready: [String: Any] = ["protocolVersion": RuntimeHandle.expectedProtocolVersion, "awaySeconds": 180]
        model.sceneChanged(to: .active)
        #expect(sent.all("app.presence").isEmpty)
        try deliver("system.ready", ready)
        #expect(sent.types.first == "app.presence")
        model.sceneChanged(to: .inactive)
        model.sceneChanged(to: .active)
        model.sceneChanged(to: .background)
        try deliver("system.ready", ready)
        model.sceneChanged(to: .background)
        model.sceneChanged(to: .inactive)
        #expect(sent.all("app.presence").compactMap { $0["state"] as? String } == ["kodosi", "away", "away", "kodosi"])
    }

    private func session(id: String) -> [String: Any] {
        [
            "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
            "programStatus": ["state": "working"], "working": true, "name": id, "isOwner": true, "status": "running",
            "attention": false, "hidden": false, "connectionState": "connected", "sharedWith": [],
        ]
    }
}

extension CommandSink {
    /// For a test that sends from the main actor alone.
    func transport(_ answer: @escaping @MainActor (Data) -> Int32?) {
        transport = { data in MainActor.assumeIsolated { answer(data) } }
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

    func all(_ type: String) -> [[String: Any]] {
        lock.withLock { values.filter { $0["type"] as? String == type } }
    }
}
