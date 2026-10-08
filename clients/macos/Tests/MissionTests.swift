import Foundation
@testable import KodosiDesktop
import Testing

@Test @MainActor func selectedMissionRefreshesAfterDirectoryInvalidation() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "MissionRefresh"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var commands: [[String: JSONValue]] = []
    app.commandSink.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            commands.append(command)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    let id = UUIDv7.generate()
    let initial: JSONValue = .object(["id": .string(id), "name": .string("Before"), "ownerUserId": .string("owner")])
    try app.receive(runtimeEvent("missions.snapshot", fields: ["missions": .array([initial]), "invitations": .array([])]))
    try app.openMission(#require(app.missions.first))
    for _ in 0 ..< 200 where !commands.contains(where: { $0["type"] == .string("mission.open") }) {
        await Task.yield()
    }
    let first = try #require(commands.last(where: { $0["type"] == .string("mission.open") }))
    try app.receive(runtimeEvent("mission.snapshot", fields: [
        "requestId": #require(first["requestId"]), "mission": initial, "members": .array([]),
    ]))
    #expect(app.missionDetail?.mission.name == "Before")
    let changed: JSONValue = .object(["id": .string(id), "name": .string("After"), "ownerUserId": .string("owner")])
    try app.receive(runtimeEvent("missions.snapshot", fields: ["missions": .array([changed]), "invitations": .array([])]))
    for _ in 0 ..< 200 where commands.filter({ $0["type"] == .string("mission.open") }).count < 2 {
        await Task.yield()
    }
    let latest = try #require(commands.last(where: { $0["type"] == .string("mission.open") }))
    #expect(latest["requestId"] != first["requestId"])
    try app.receive(runtimeEvent("mission.snapshot", fields: [
        "requestId": #require(first["requestId"]), "mission": initial, "members": .array([]),
    ]))
    #expect(app.missionDetail == nil)
    try app.receive(runtimeEvent("mission.snapshot", fields: [
        "requestId": #require(latest["requestId"]), "mission": changed, "members": .array([]),
    ]))
    #expect(app.missionDetail?.mission.name == "After")
    try app.receive(runtimeEvent("missions.snapshot", fields: [
        "missions": .array([]), "invitations": .array([]), "missionsTruncated": .bool(true),
    ]))
    #expect(app.missionListTruncated)
    #expect(app.workbench.selectedMissionId == id)
    for _ in 0 ..< 200 where commands.filter({ $0["type"] == .string("mission.open") }).count < 3 {
        await Task.yield()
    }
    let offPage = try #require(commands.last(where: { $0["type"] == .string("mission.open") }))
    try app.receive(runtimeEvent("mission.snapshot", fields: [
        "requestId": #require(offPage["requestId"]), "mission": changed, "members": .array([]),
    ]))
    #expect(app.missionDetail?.mission.name == "After")
    try app.receive(runtimeEvent("missions.snapshot", fields: ["missions": .array([]), "invitations": .array([])]))
    #expect(!app.missionListTruncated)
    #expect(app.missionDetail == nil)
    #expect(app.workbench.selectedMissionId == nil)
}
