import Foundation
@testable import KodosiDesktop
import Testing

@Test @MainActor func aTerminalOnANewBranchSendsTheBranchAndTheChosenFolder() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "BranchTerminal"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport = { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data), fields["type"] == .string("session.create") {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(54)]))

    func create(branch: String?) async throws {
        let creating = Task { try await app.createSession(name: "Test", directory: "/tmp/project", branch: branch) }
        let count = sent.count
        for _ in 0 ..< 100 where sent.count == count {
            await Task.yield()
        }
        let request = try #require(sent.last?["requestId"]?.stringValue)
        try app.receive(runtimeEvent("session.result", epoch: 0, userId: nil, fields: [
            "requestId": .string(request), "sessionId": .string(UUIDv7.generate()), "operation": .string("session.create"),
        ]))
        try await creating.value
    }

    try await create(branch: "fix/login")
    #expect(sent.last?["branch"] == .string("fix/login"))
    #expect(sent.last?["worktrees"] == nil)

    app.settings.branchFolder = "/tmp/branches"
    try await create(branch: "fix/logout")
    #expect(sent.last?["worktrees"] == .string("/tmp/branches"))

    try await create(branch: nil)
    #expect(sent.last?["branch"] == nil)
    #expect(sent.last?["worktrees"] == nil)
}

@Test @MainActor func theBranchFolderStaysAfterARestart() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "BranchFolder"))
    let settings = DesktopSettings(defaults: defaults)
    #expect(settings.branchFolder == nil)
    settings.branchFolder = "/tmp/branches"
    settings.save()
    #expect(DesktopSettings(defaults: defaults).branchFolder == "/tmp/branches")
    settings.branchFolder = nil
    settings.save()
    #expect(DesktopSettings(defaults: defaults).branchFolder == nil)
}
