import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import KodosiTerminalSessions
import Testing

@MainActor
struct TerminalControllerTests {
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
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s1", state: "working")]])
        log.clear()
    }

    @Test func `opening announces the view and leaving disconnects it`() async {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        await log.waitFor("session.openRemote")
        #expect(log.fields(of: "session.openRemote").map { $0["sessionId"] as? String } == ["s1"])
        #expect(model.terminals.watchingSessionId == "s1")
        controller.disappear()
        #expect(log.fields(of: "session.disconnect").map { $0["sessionId"] as? String } == ["s1"])
        #expect(model.terminals.watchingSessionId == nil)
        #expect(!controller.opening)
    }

    @Test func `a terminal without access is not opened`() async throws {
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s1", state: "working", connection: "blocked")]])
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        for _ in 0 ..< 20 {
            await Task.yield()
        }
        #expect(log.fields(of: "session.openRemote").isEmpty)
        controller.disappear()
    }

    @Test func `an open terminal moves to the terminal that its computer started again`() async throws {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        await log.waitFor("session.openRemote")
        try deliver("sessions.snapshot", ["sessions": []])
        controller.refresh()
        #expect(controller.session == nil)
        log.clear()
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s2", state: "working", restoredFrom: "s1")]])
        controller.refresh()
        #expect(controller.sessionId == "s2")
        #expect(log.fields(of: "session.disconnect").map { $0["sessionId"] as? String } == ["s1"])
        await log.waitFor("session.openRemote")
        #expect(log.fields(of: "session.openRemote").map { $0["sessionId"] as? String } == ["s2"])
        #expect(model.terminals.watchingSessionId == "s2")
        controller.disappear()
    }

    @Test func `a terminal moves only to a restart of itself on the same computer of the same person`() throws {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        try deliver("sessions.snapshot", ["sessions": [
            terminalEntry("elsewhere", state: "working", host: "Air", restoredFrom: "s1"),
            terminalEntry("someone", state: "working", owner: false, restoredFrom: "s1"),
            terminalEntry("other", state: "working", restoredFrom: "s9"),
        ]])
        controller.refresh()
        #expect(controller.sessionId == "s1")
        controller.noteEnding()
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s2", state: "working", restoredFrom: "s1")]])
        controller.refresh()
        #expect(controller.sessionId == "s1")
        controller.disappear()
    }

    @Test func `background stops watching and tells the host`() throws {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        try deliver("term.bell", ["sessionId": "s1"])
        #expect(controller.bells == 1)
        controller.setActive(false)
        #expect(model.terminals.watchingSessionId == nil)
        #expect(log.fields(of: "session.visible").last?["sessionIds"] as? [String] == [])
        try deliver("term.bell", ["sessionId": "s1"])
        #expect(controller.bells == 1)
        controller.setActive(true)
        #expect(log.fields(of: "session.visible").last?["sessionIds"] as? [String] == ["s1"])
        controller.disappear()
    }

    @Test func `focus and blur name the view subscription`() throws {
        let token = TerminalConnectionToken()
        let surface = UUID()
        let focus = TerminalProcessFocusCoordinator(commands: model.commands)
        focus.acquire(
            sessionId: "s1", runtimeIncarnationId: "s1-1", subscriptionId: token.subscriptionId,
            subscriptionGeneration: token.subscriptionGeneration, surfaceId: surface,
        )
        let focused = try #require(log.fields(of: "session.focus").first)
        #expect(focused["sessionId"] as? String == "s1")
        #expect(focused["clientId"] as? String == token.subscriptionId)
        #expect((focused["subscriptionGeneration"] as? NSNumber)?.uint64Value == token.subscriptionGeneration)
        #expect(focused["expectedRuntimeIncarnationId"] as? String == "s1-1")
        #expect((focused["requestId"] as? String)?.isEmpty == false)
        focus.release(surfaceId: surface)
        let blurred = try #require(log.fields(of: "session.blur").first)
        #expect(blurred["clientId"] as? String == token.subscriptionId)
        #expect((blurred["subscriptionGeneration"] as? NSNumber)?.uint64Value == token.subscriptionGeneration)
        #expect(blurred["expectedRuntimeIncarnationId"] as? String == "s1-1")
        #expect((blurred["requestId"] as? String)?.isEmpty == false)
    }

    @Test func `only a visible active rendered controllable view takes focus`() {
        let focused = TerminalPresence(visible: true, active: true, rendered: true, canControl: true)
        #expect(focused.watching)
        #expect(focused.focused)
        var presence = focused
        presence.active = false
        #expect(!presence.watching)
        #expect(!presence.focused)
        presence = focused
        presence.rendered = false
        #expect(presence.watching)
        #expect(!presence.focused)
        presence = focused
        presence.canControl = false
        #expect(!presence.focused)
        presence = focused
        presence.visible = false
        #expect(!presence.watching)
    }

    @Test func `a message waits while the terminal cannot take it`() {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        controller.draft = "Plan the release\nThen ship it"
        #expect(!controller.send())
        #expect(controller.draft == "Plan the release\nThen ship it")
        #expect(controller.sent == 0)
        controller.disappear()
    }

    @Test func `interrupt, leave and hide name the current incarnation`() async throws {
        let session = try #require(model.terminals.session("s1"))
        model.terminals.interrupt(session)
        model.terminals.leave(session)
        model.terminals.hide(session)
        await log.waitFor("session.hide")
        for type in ["session.interrupt", "session.leave", "session.hide"] {
            let fields = try #require(log.fields(of: type).first)
            #expect(fields["sessionId"] as? String == "s1")
            #expect(fields["expectedRuntimeIncarnationId"] as? String == "s1-1")
            try deliver("session.result", ["requestId": fields["requestId"] ?? "", "operation": type])
        }
    }

    @Test func `sharing with a room attaches the terminal`() async throws {
        let session = try #require(model.terminals.session("s1"))
        let sharing = Task { try await model.terminals.share(session, inRoom: "room-1") }
        await log.waitFor("session.attachMission")
        let fields = try #require(log.fields(of: "session.attachMission").first)
        #expect(fields["missionId"] as? String == "room-1")
        #expect(fields["expectedRuntimeIncarnationId"] as? String == "s1-1")
        try deliver("session.result", ["requestId": fields["requestId"] ?? "", "operation": "session.attachMission"])
        try await sharing.value
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

func terminalEntry(
    _ id: String,
    state: String,
    owner: Bool = true,
    host: String = "Studio",
    connection: String = "connected",
    room: (id: String, name: String)? = nil,
    restoredFrom: String? = nil,
) -> [String: Any] {
    var entry: [String: Any] = [
        "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
        "programStatus": ["state": state], "name": id, "isOwner": owner, "status": "running",
        "attention": false, "hidden": false, "connectionState": connection, "sharedWith": [], "hostName": host, "hostDeviceId": "device-\(host)",
        "ownerUserId": owner ? "user-1" : "user-2", "ownerName": owner ? "Me" : "Maria",
    ]
    if let room {
        entry["missionId"] = room.id
        entry["missionName"] = room.name
    }
    if let restoredFrom {
        entry["restoredFrom"] = restoredFrom
    }
    return entry
}

final nonisolated class TerminalCommandLog: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [[String: Any]] = []

    func append(_ data: Data) {
        guard let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        lock.withLock { values.append(value) }
    }

    func clear() {
        lock.withLock { values.removeAll() }
    }

    func fields(of type: String) -> [[String: Any]] {
        lock.withLock { values.filter { $0["type"] as? String == type } }
    }

    @MainActor
    func waitFor(_ type: String) async {
        for _ in 0 ..< 200 where fields(of: type).isEmpty {
            await Task.yield()
        }
    }
}
