import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test @MainActor func `a project that the person left is no longer shown`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "ProjectLeft"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    let project: JSONValue = .object(["id": .string("project-1"), "name": .string("Launch"), "ownerUserId": .string("owner"), "unread": .bool(false)])
    try app.receive(runtimeEvent("projects.snapshot", fields: ["projects": .array([project]), "invitations": .array([])]))
    app.workbench.section = .projects
    app.openProject(id: "project-1")
    try app.receive(runtimeEvent("projects.snapshot", fields: [
        "projects": .array([]), "invitations": .array([]), "projectsTruncated": .bool(true),
    ]))
    #expect(app.projects.listTruncated)
    #expect(app.workbench.selectedProjectId == "project-1")
    try app.receive(runtimeEvent("projects.snapshot", fields: ["projects": .array([]), "invitations": .array([])]))
    #expect(!app.projects.listTruncated)
    #expect(app.workbench.selectedProjectId == nil)
}
