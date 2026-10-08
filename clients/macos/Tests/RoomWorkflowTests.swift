import Foundation
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
