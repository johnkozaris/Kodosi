import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

struct TerminalControllerTests {
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
        try deliver("sessions.snapshot", ["sessions": [terminalEntry("s1", state: "working")]])
        log.clear()
    }

    @Test func `opening announces the view and leaving disconnects it`() async {
        let controller = TerminalController(sessionId: "s1", store: model.terminals, lock: model.lock)
        controller.appear(active: true)
        await log.waitFor("session.openRemote")
        #expect(log.fields(of: "session.openRemote").map { $0["sessionId"] as? String } == ["s1"])
        #expect(model.terminals.watchingSessionId == "s1")
        controller.disappear()
        #expect(log.fields(of: "session.disconnect").map { $0["sessionId"] as? String } == ["s1"])
        #expect(model.terminals.watchingSessionId == nil)
        #expect(!controller.opening)
    }

    @Test func `an open terminal moves to the terminal that its computer started again`() async throws {
        let controller = TerminalController(sessionId: "s1", store: model.terminals, lock: model.lock)
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
        let controller = TerminalController(sessionId: "s1", store: model.terminals, lock: model.lock)
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

    @Test func `background stops watching and tells the host`() {
        let controller = TerminalController(sessionId: "s1", store: model.terminals, lock: model.lock)
        controller.appear(active: true)
        #expect(model.terminals.watchingSessionId == "s1")
        controller.setActive(false)
        #expect(model.terminals.watchingSessionId == nil)
        #expect(log.fields(of: "session.visible").last?["sessionIds"] as? [String] == [])
        controller.setActive(true)
        #expect(model.terminals.watchingSessionId == "s1")
        #expect(log.fields(of: "session.visible").last?["sessionIds"] as? [String] == ["s1"])
        controller.disappear()
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
    restoredFrom: String? = nil,
) -> [String: Any] {
    var entry: [String: Any] = [
        "id": id, "incarnationId": "\(id)-1", "kind": "remote", "program": "claude",
        "programStatus": ["state": state], "working": state == "working", "name": id, "isOwner": owner, "status": "running",
        "attention": false, "hidden": false, "connectionState": "connected", "sharedWith": [], "hostName": host, "hostDeviceId": "device-\(host)",
        "ownerUserId": owner ? "user-1" : "user-2", "ownerName": owner ? "Me" : "Maria",
    ]
    if ["blocked", "done", "error"].contains(state) {
        entry["waiting"] = state
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
