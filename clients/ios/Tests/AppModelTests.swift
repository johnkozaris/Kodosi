import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiDesign
import KodosiKit
import KodosiTerminalSessions
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

    @Test func `an empty terminal list counts once the server answered`() throws {
        try deliver("system.ready", ["protocolVersion": RuntimeHandle.expectedProtocolVersion])
        try deliver("sessions.snapshot", ["sessions": [Any]()])
        #expect(model.terminals.loaded)
        #expect(!model.terminals.listed)
        let request = try #require(sent.last("session.list")?["requestId"] as? String)
        try deliver("session.result", ["requestId": request, "operation": "session.list"])
        #expect(model.terminals.listed)
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

    @Test func `a refresh that the app asked for by itself fails without a notice`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        for operation in RuntimeEvent.refreshes {
            let area = operation.prefix(while: { $0 != "." })
            try deliver("\(area).error", ["operation": operation, "message": "Kodosi cannot reach the server."])
        }
        #expect(model.notices.current == nil)
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

    @Test func `the frames of a live document reach its view`() async throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "working")]])
        let item = CanvasItem(key: "notes", name: "Notes.docx", kind: .document, view: .docx, revision: 1, bytes: 1, changed: 1)
        let link = model.terminals.canvas.document(item, in: "s1")
        var frames: [Data] = []
        link.onFrame = { frames.append($0) }
        let opening = Task { try await link.open() }
        for _ in 0 ..< 100 where sent.last("document.open") == nil {
            await Task.yield()
        }
        let open = try #require(sent.last("document.open"))
        try deliver("document.result", ["requestId": #require(open["requestId"] as? String), "operation": "document.open"])
        try await opening.value
        let pipe = try #require(link.pipe)
        try deliver("document.frame", ["pipe": pipe, "sequence": 0, "more": true, "data": Data("he".utf8).base64EncodedString()])
        try deliver("document.frame", ["pipe": pipe, "sequence": 1, "more": false, "data": Data("llo".utf8).base64EncodedString()])
        #expect(frames == [Data("hello".utf8)])
    }

    @Test func `a picture sent to another computer shows how much arrived`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [session(id: "s1", state: "working")]])
        _ = model.terminals.live.session(for: "s1")
        try deliver("session.picture", ["sessionId": "s1", "state": "sending", "sent": 3, "total": 10])
        #expect(model.terminals.live.picture(for: "s1")?.sent == 3)
    }

    @Test func `while Kodosi is offline a terminal of another computer says Offline, unless it has no access`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        var reconnecting = terminalEntry("reconnecting", state: "working")
        reconnecting["status"] = "reconnecting"
        var away = terminalEntry("away", state: "idle", connection: "offline")
        away["status"] = "offline"
        try deliver("sessions.snapshot", ["sessions": [reconnecting, away, terminalEntry("locked", state: "idle", connection: "blocked")]])
        let terminals = try ["reconnecting", "away", "locked"].map { try #require(model.terminals.session($0)) }
        #expect(terminals.map(model.statusLabel(of:)) == ["Reconnecting", "Studio is offline", "No access"])
        try deliver("connection.state", ["online": false])
        #expect(terminals.map(model.statusLabel(of:)) == ["Offline", "Offline", "No access"])
        try deliver("connection.result", ["requestId": "retry", "operation": "connection.retry"])
        try deliver("connection.state", ["online": true])
        #expect(terminals.map(model.statusLabel(of:)) == ["Reconnecting", "Studio is offline", "No access"])
    }

    @Test func `the computer that answered last answers Dosi, or else the first that runs Kodosi`() throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("devices.list", ["selfDeviceId": "phone", "localDeviceEnrolled": true, "devices": [
            device("phone", "iPhone"), device("studio", "Studio"), device("air", "Air"), device("key", "Recovery key", recovery: true),
        ]])
        #expect(model.dosiComputer == nil)
        try deliver("devices.hosts", ["deviceIds": ["phone", "studio", "air", "key"]])
        #expect(model.dosiComputers.map(\.name) == ["Air", "Studio"])
        #expect(model.dosiComputer?.name == "Air")
        try deliver("dosi.snapshot", ["dosi": dosi(last: ["device": "studio", "name": "Studio"])])
        #expect(model.dosiComputer?.name == "Studio")
        try deliver("devices.hosts", ["deviceIds": ["phone", "air"]])
        let studio = try #require(model.dosiComputer)
        #expect(model.dosiComputers.map(\.name) == ["Air", "Studio"])
        #expect(studio.name == "Studio" && model.dosiAway(studio))
        model.dosiDevice = "air"
        #expect(model.dosiComputer?.name == "Air")
        try deliver("devices.hosts", ["deviceIds": ["phone"]])
        #expect(model.dosiComputers.map(\.name) == ["Studio"])
        #expect(model.dosiComputer?.name == "Studio")
    }

    @Test func `a message to Dosi goes to the chosen computer, and another account starts clean`() async throws {
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("devices.list", ["selfDeviceId": "phone", "localDeviceEnrolled": true, "devices": [
            device("phone", "iPhone"), device("studio", "Studio"),
        ]])
        try deliver("devices.hosts", ["deviceIds": ["studio"]])
        try deliver("dosi.snapshot", ["dosi": dosi(state: "connecting")])
        #expect(!model.dosiConnected)
        try deliver("dosi.snapshot", ["dosi": dosi()])
        #expect(model.dosiConnected)
        model.dosi.draft = "  What needs me?  "
        model.sendToDosi()
        #expect(model.dosi.draft.isEmpty)
        for _ in 0 ..< 200 where sent.last("dosi.send") == nil {
            await Task.yield()
        }
        let send = try #require(sent.last("dosi.send"))
        #expect(send["text"] as? String == "What needs me?" && send["device"] as? String == "studio")
        try deliver("dosi.item", ["item": ["id": "a", "turn": "1", "at": 1, "body": ["kind": "person", "text": "What needs me?"]]])
        #expect(model.dosi.items.map(\.id) == ["a"])
        model.dosiDevice = "studio"
        try deliver("auth.ready", userId: "user-2", epoch: 2, ["userId": "user-2", "enrolled": true])
        #expect(model.dosi.snapshot == nil && model.dosi.items.isEmpty && model.dosiDevice == nil)
    }

    private func device(_ id: String, _ label: String, recovery: Bool = false) -> [String: Any] {
        ["deviceId": id, "label": label, "certIssuedAtMs": 1, "recoveryKey": recovery]
    }

    private func dosi(state: String = "idle", last: [String: Any]? = nil) -> [String: Any] {
        [
            "state": state, "setup": ["agent": NSNull(), "picks": [Any]()], "agents": [Any](), "instructionsEdited": false,
            "folder": "", "shared": true, "remote": NSNull(), "conversation": NSNull(), "items": [Any](), "earlier": false,
            "queued": [Any](), "asks": [Any](), "last": last ?? NSNull(),
        ]
    }

    private func session(id: String, state: String) -> [String: Any] {
        [
            "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
            "programStatus": ["state": state], "name": id, "isOwner": true, "status": "running",
            "attention": false, "hidden": false, "connectionState": "connected", "sharedWith": [],
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
