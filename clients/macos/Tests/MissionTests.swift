import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test @MainActor func `a room that the person left is no longer shown`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "MissionLeft"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    let room: JSONValue = .object(["id": .string("room-1"), "name": .string("Launch"), "ownerUserId": .string("owner"), "unread": .bool(false)])
    try app.receive(runtimeEvent("missions.snapshot", fields: ["missions": .array([room]), "invitations": .array([])]))
    app.workbench.section = .missions
    app.openMission(id: "room-1")
    try app.receive(runtimeEvent("missions.snapshot", fields: [
        "missions": .array([]), "invitations": .array([]), "missionsTruncated": .bool(true),
    ]))
    #expect(app.rooms.listTruncated)
    #expect(app.workbench.selectedMissionId == "room-1")
    try app.receive(runtimeEvent("missions.snapshot", fields: ["missions": .array([]), "invitations": .array([])]))
    #expect(!app.rooms.listTruncated)
    #expect(app.workbench.selectedMissionId == nil)
}
