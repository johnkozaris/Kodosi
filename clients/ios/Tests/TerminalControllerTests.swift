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
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s1", state: "working")]])
        log.clear()
    }

    @Test func openingAnnouncesTheViewAndLeavingDisconnectsIt() async {
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

    @Test func aTerminalWithoutAccessIsNotOpened() async throws {
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s1", state: "working", connection: "blocked")]])
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        for _ in 0 ..< 20 {
            await Task.yield()
        }
        #expect(log.fields(of: "session.openRemote").isEmpty)
        controller.disappear()
    }

    @Test func backgroundStopsWatchingSoBellsBecomeAttention() throws {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        try deliver("term.bell", ["sessionId": "s1"])
        #expect(model.terminals.attention.isEmpty)
        #expect(controller.bells == 1)
        controller.setActive(false)
        #expect(model.terminals.watchingSessionId == nil)
        try deliver("term.bell", ["sessionId": "s1"])
        #expect(model.terminals.attention == ["s1"])
        controller.setActive(true)
        #expect(model.terminals.watchingSessionId == "s1")
        #expect(model.terminals.attention.isEmpty)
        controller.disappear()
    }

    @Test func focusAndBlurNameTheViewSubscription() throws {
        let token = TerminalConnectionToken()
        let surface = UUID()
        let focus = TerminalProcessFocusCoordinator(commands: model.commands)
        focus.acquire(
            sessionId: "s1", runtimeIncarnationId: "s1-1", subscriptionId: token.subscriptionId,
            subscriptionGeneration: token.subscriptionGeneration, surfaceId: surface
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
        #expect(blurred["requestId"] == nil)
    }

    @Test func thePhoneClaimsTheSizeOnlyWhileItHoldsFocus() {
        let live = model.terminals.live
        #expect(!live.claimsSize(for: "s1"))
        live.setClaimsSize(true, for: "s1")
        #expect(live.claimsSize(for: "s1"))
        #expect(!live.claimsSize(for: "s2"))
        live.setClaimsSize(false, for: "s1")
        #expect(!live.claimsSize(for: "s1"))
    }

    @Test func onlyAVisibleActiveRenderedControllableViewTakesFocus() {
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

    @Test func aMessageWaitsWhileTheTerminalCannotTakeIt() {
        let controller = TerminalController(sessionId: "s1", store: model.terminals)
        controller.appear(active: true)
        controller.draft = "Plan the release\nThen ship it"
        controller.send()
        #expect(controller.draft == "Plan the release\nThen ship it")
        #expect(controller.sent == 0)
        controller.disappear()
    }

    @Test func interruptAndLeaveNameTheCurrentIncarnation() async throws {
        let session = try #require(model.terminals.session("s1"))
        model.terminals.interrupt(session)
        model.terminals.leave(session)
        await log.waitFor("session.leave")
        for type in ["session.interrupt", "session.leave"] {
            let fields = try #require(log.fields(of: type).first)
            #expect(fields["sessionId"] as? String == "s1")
            #expect(fields["expectedRuntimeIncarnationId"] as? String == "s1-1")
            try deliver("session.result", ["requestId": fields["requestId"] ?? "", "operation": type])
        }
    }

    @Test func sharingWithARoomAttachesTheTerminal() async throws {
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
    room: (id: String, name: String)? = nil
) -> [String: Any] {
    var entry: [String: Any] = [
        "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
        "programStatus": ["state": state], "name": id, "isOwner": owner, "status": "running",
        "connectionState": connection, "sharedWith": [], "hostName": host, "hostDeviceId": "device-\(host)",
        "ownerName": owner ? "Me" : "Maria",
    ]
    if let room {
        entry["missionId"] = room.id
        entry["missionName"] = room.name
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
