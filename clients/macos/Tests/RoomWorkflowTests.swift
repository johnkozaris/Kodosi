import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test @MainActor func aSendAcknowledgementPreservesTheNextRoomDraft() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "RoomDraft"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    let roomId = UUIDv7.generate()
    var sent: [String: JSONValue]?
    app.commandSink.transport = { data in
        sent = try? JSONDecoder().decode([String: JSONValue].self, from: data)
        return 0
    }
    let state = app.roomView(roomId)
    state.message = "First message"
    let delivery = Task { await app.postRoomMessage(roomId) }
    for _ in 0 ..< 10 where sent == nil {
        await Task.yield()
    }
    let command = try #require(sent)
    #expect(command["type"]?.stringValue == "room.command")
    state.message = "Next thought, still being written"
    try app.commandSink.receive(runtimeEvent("room.result", epoch: 0, userId: nil, fields: [
        "requestId": #require(command["requestId"]), "operation": .string("room.command"),
        "roomId": .string(roomId), "action": .string("post"), "itemId": .string(UUIDv7.generate()),
    ]))
    await delivery.value
    #expect(state.message == "Next thought, still being written")
    #expect(!state.busy)
    #expect(app.roomView(roomId) === state)
    #expect(app.roomView(UUIDv7.generate()).message.isEmpty)
}

@Test @MainActor func aRoomCountsOnlyNewMessagesThatSomeoneElseWrote() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "RoomUnread"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: "user-1", fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    func message(_ id: String, _ sequence: UInt64, author: String, agent: String? = nil) -> JSONValue {
        .object([
            "id": .string(id), "sequence": .uint(sequence), "authorId": .string(author), "authorName": .string(author),
            "agent": agent.map(JSONValue.string) ?? .null, "terminalId": .null, "text": .string("Hello"),
            "createdAt": .string("2026-10-15T10:00:00Z"),
        ])
    }
    func snapshot(_ messages: [JSONValue]) throws -> RuntimeEvent {
        try runtimeEvent("room.snapshot", epoch: 0, userId: "user-1", fields: ["room": .object([
            "roomId": .string("room-1"), "messages": .array(messages), "tasks": .array([]), "repositories": .array([]),
            "sequence": .uint(UInt64(messages.count)), "hasOlder": .bool(true), "historyTrimmed": .bool(false),
        ])])
    }
    try app.receive(snapshot([message("m5", 5, author: "user-2")]))
    #expect(app.roomView("room-1").unread == 0)
    try app.receive(snapshot([
        message("m3", 3, author: "user-2"), message("m6", 6, author: "user-1"),
        message("m7", 7, author: "user-1", agent: "Claude Code"), message("m8", 8, author: "user-2"),
    ]))
    #expect(app.roomView("room-1").unread == 2)
}

@Test @MainActor func aShownRoomIsSeenWhenTheAppComesToTheFront() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "RoomSeen"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var seen: [String: JSONValue]?
    app.commandSink.transport = { data in
        let command = try? JSONDecoder().decode([String: JSONValue].self, from: data)
        if command?["type"]?.stringValue == "mission.seen" {
            seen = command
        }
        return 0
    }
    var front = false
    app.isFront = { front }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: "user-1", fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    app.workbench.section = .missions
    app.workbench.selectedMissionId = "room-1"
    try app.receive(runtimeEvent("room.snapshot", epoch: 0, userId: "user-1", fields: ["room": .object([
        "roomId": .string("room-1"), "tasks": .array([]), "repositories": .array([]),
        "messages": .array([.object([
            "id": .string("m4"), "sequence": .uint(4), "authorId": .string("user-2"), "authorName": .string("Leo"),
            "agent": .null, "terminalId": .null, "text": .string("Hello"), "createdAt": .string("2026-10-15T10:00:00Z"),
        ])]),
        "sequence": .uint(4), "hasOlder": .bool(false), "historyTrimmed": .bool(false),
    ])]))
    #expect(seen == nil)
    front = true
    app.seeShownRoom()
    #expect(seen?["missionId"]?.stringValue == "room-1")
    #expect(seen?["sequence"] == .int(4))
}
