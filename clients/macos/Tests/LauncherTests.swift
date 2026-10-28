import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@MainActor
private func launcherApp(_ prefix: String) throws -> (AppDependencies, () -> [[String: JSONValue]]) {
    let defaults = try #require(EphemeralUserDefaults(prefix: prefix))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport = { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    return (app, { sent })
}

private let catalog: JSONValue = .object([
    "kodosi": .null,
    "choice": .object([
        "agent": .string("claude"),
        "picks": .array([
            .object(["agent": .string("claude"), "account": .string("work"), "model": .null, "effort": .string("high")]),
        ]),
    ]),
    "agents": .array([
        .object([
            "agent": .string("claude"), "program": .string("claude"), "installed": .bool(true), "install": .string(""),
            "addsAccounts": .bool(true), "reportsUsage": .bool(true),
            "accounts": .array([
                .object(["name": .string("default"), "command": .string("claude"), "identity": .string("ada@example.com"),
                         "folder": .string("~/.claude")]),
                .object(["name": .string("work"), "command": .string("env CLAUDE_CONFIG_DIR=/Users/ada/.claude-work claude"),
                         "identity": .null, "folder": .string("~/.claude-work")]),
            ]),
        ]),
        .object([
            "agent": .string("codex"), "program": .string("codex"), "installed": .bool(false),
            "install": .string("curl -fsSL https://chatgpt.com/codex/install.sh | sh"), "addsAccounts": .bool(true),
            "reportsUsage": .bool(true),
            "accounts": .array([]),
        ]),
    ]),
])

@MainActor
private func answerAgentRequests(_ app: AppDependencies, sent: () -> [[String: JSONValue]]) async throws {
    var answered = Set<String>()
    for _ in 0 ..< 500 where app.launcher.catalog == nil {
        await Task.yield()
        let requests = sent().filter { $0["type"] == .string("provider.agents") }.compactMap { $0["requestId"]?.stringValue }
        for request in requests where answered.insert(request).inserted {
            try app.receive(runtimeEvent("provider.result", epoch: 0, userId: nil, fields: [
                "requestId": .string(request), "operation": .string("provider.agents"), "result": catalog,
            ]))
        }
    }
}

@Test @MainActor func `new terminal focuses the home launcher and opens a sheet elsewhere`() throws {
    let (app, _) = try launcherApp("LauncherHome")
    defer { app.shutdownProcess() }
    app.openLauncher()
    #expect(app.workbench.launcher == nil)
    #expect(app.workbench.launcherFocus == 1)
    app.workbench.launcherDraft.folder = "/work/atlas"
    app.openLauncher(roomId: "room")
    #expect(app.workbench.launcher == LauncherRequest(roomId: "room"))
    app.workbench.launcherDraft.prompt = "fix the build"
    app.workbench.launcherDraft.remoteFolders["studio"] = "/Users/me/atlas"
    app.workbench.launcher = nil
    app.workbench.section = .people
    app.openLauncher()
    #expect(app.workbench.launcher == LauncherRequest())
    #expect(
        app.workbench.launcherDraft
            == LauncherDraft(prompt: "fix the build", folder: "/work/atlas", remoteFolders: ["studio": "/Users/me/atlas"]),
    )
    app.workbench.switchAccount("another-user")
    #expect(app.workbench.launcherDraft == LauncherDraft())
}

@Test @MainActor func `a terminal started for a room joins that room`() async throws {
    let (app, sent) = try launcherApp("LauncherRoom")
    defer { app.shutdownProcess() }
    let creating = Task { try await app.createSession(name: "Test", directory: "/work", roomId: "room", command: "claude") }
    var create: [String: JSONValue]?
    for _ in 0 ..< 200 where create == nil {
        await Task.yield()
        create = sent().last { $0["type"] == .string("session.create") }
    }
    let fields = try #require(create)
    #expect(fields["missionId"] == .string("room"))
    #expect(fields["command"] == .string("claude"))
    let request = try #require(fields["requestId"]?.stringValue)
    let created = UUIDv7.generate()
    try app.receive(runtimeEvent("session.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(request), "sessionId": .string(created), "operation": .string("session.create"),
    ]))
    try await creating.value
    #expect(app.freshTerminals.contains(created))
}

@Test @MainActor func `a folder's plus starts a plain terminal there without the launcher`() async throws {
    let (app, sent) = try launcherApp("LauncherFolder")
    defer { app.shutdownProcess() }
    app.openTerminal(in: "/work/atlas")
    var create: [String: JSONValue]?
    for _ in 0 ..< 200 where create == nil {
        await Task.yield()
        create = sent().last { $0["type"] == .string("session.create") }
    }
    let fields = try #require(create)
    #expect(fields["workingDir"] == .string("/work/atlas"))
    #expect(fields["agent"] == nil)
    #expect(fields["command"] == nil)
    #expect(app.workbench.launcher == nil)
    #expect(app.workbench.launcherFocus == 0)
    let created = UUIDv7.generate()
    try app.receive(runtimeEvent("session.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(#require(fields["requestId"]?.stringValue)), "sessionId": .string(created),
        "operation": .string("session.create"),
    ]))
    for _ in 0 ..< 200 where !app.freshTerminals.contains(created) {
        await Task.yield()
    }
    #expect(app.freshTerminals.contains(created))
}

@Test @MainActor func `a launcher for another computer asks that computer and starts there`() async throws {
    let (app, sent) = try launcherApp("LauncherComputer")
    defer { app.shutdownProcess() }
    let studio = app.launcher(for: "studio")
    #expect(app.launcher(for: "studio") === studio)
    #expect(app.launcher(for: nil) === app.launcher)
    Task { _ = try? await studio.folders(at: "/home/maya") }
    var listed: [String: JSONValue]?
    for _ in 0 ..< 200 where listed == nil {
        await Task.yield()
        listed = sent().last { $0["type"] == .string("folders.list") }
    }
    #expect(listed?["computer"] == .string("studio"))
    #expect(listed?["path"] == .string("/home/maya"))
    app.settings.lastWorkingDir = "/Users/maya/here"
    let creating = Task { try await app.createSession(name: "Test", directory: "/home/maya/work", computer: "studio") }
    var create: [String: JSONValue]?
    for _ in 0 ..< 200 where create == nil {
        await Task.yield()
        create = sent().last { $0["type"] == .string("session.create") }
    }
    let fields = try #require(create)
    #expect(fields["computer"] == .string("studio"))
    #expect(fields["workingDir"] == .string("/home/maya/work"))
    try app.receive(runtimeEvent("session.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(#require(fields["requestId"]?.stringValue)), "sessionId": .string(UUIDv7.generate()),
        "operation": .string("session.create"),
    ]))
    try await creating.value
    #expect(app.settings.lastWorkingDir == "/Users/maya/here")
}

@Test @MainActor func `the folder menu keeps the host's recent folders after a restart`() async throws {
    let (app, sent) = try launcherApp("LauncherRecent")
    defer { app.shutdownProcess() }
    app.settings.lastWorkingDir = "/work/atlas"
    let listing = Task { try await app.launcher.folders() }
    var request: String?
    for _ in 0 ..< 200 where request == nil {
        await Task.yield()
        request = sent().last { $0["type"] == .string("folders.list") }?["requestId"]?.stringValue
    }
    #expect(sent().last { $0["type"] == .string("folders.list") }?["computer"] == nil)
    try app.receive(runtimeEvent("folders.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(#require(request)), "operation": .string("folders.list"),
        "folders": .object([
            "path": .string("/Users/maya"), "parent": .string("/Users"), "home": .string("/Users/maya"), "folders": .array([]),
            "recent": .array(["/work/kodosi", "/work/atlas", "/work/notes"].map(JSONValue.string)),
        ]),
    ]))
    _ = try await listing.value
    let home = [app.storageBootstrap.defaultWorkingDirectory?.path].compactMap(\.self)
    #expect(recentFolders(app, chosen: nil, saved: app.launcher.places?.recent ?? [])
        == ["/work/atlas", "/work/kodosi", "/work/notes"] + home)
}

@Test func `a folder of another computer shows from its own home`() {
    let mac = FolderList(path: "/home/maya/Repos", parent: "/home/maya", home: "/home/maya", folders: ["kodosi"], recent: [])
    #expect(mac.shown("/home/maya") == "~")
    #expect(mac.shown("/home/maya/Repos/kodosi") == "~/Repos/kodosi")
    #expect(mac.shown("/home/mayabe") == "/home/mayabe")
    #expect(mac.child("kodosi") == "/home/maya/Repos/kodosi")
    let windows = FolderList(path: #"C:\Users\maya"#, parent: #"C:\Users"#, home: #"C:\Users\maya"#, folders: [], recent: [])
    #expect(windows.shown(#"C:\Users\maya\Repos"#) == #"~\Repos"#)
    #expect(windows.child("Repos") == #"C:\Users\maya\Repos"#)
}

@Test @MainActor func `the launcher starts the agent as it shows it`() async throws {
    let (app, sent) = try launcherApp("LauncherPlan")
    defer { app.shutdownProcess() }
    try await answerAgentRequests(app, sent: sent)
    #expect(app.launcher.installed.map(\.agent) == ["claude"])
    #expect(LaunchPlan(launcher: app.launcher).start == AgentStart(agent: "claude", account: "work", model: "", effort: "high"))
    #expect(LaunchPlan(launcher: app.launcher, agent: "codex").start == nil)
    app.launcher.choose(.pick(AgentPick(agent: "claude", account: "gone")))
    #expect(LaunchPlan(launcher: app.launcher).account?.name == "default")
}

@Test @MainActor func `usage shows only on accounts that are signed in`() async throws {
    let (app, sent) = try launcherApp("LauncherUsage")
    defer { app.shutdownProcess() }
    try await answerAgentRequests(app, sent: sent)
    #expect(app.launcher.usageAccounts.map(\.account.name) == ["default"])
    let reading = Task { await app.launcher.refreshUsage() }
    var asked: String?
    for _ in 0 ..< 200 where asked == nil {
        await Task.yield()
        asked = sent().last { $0["type"] == .string("provider.usage") }?["requestId"]?.stringValue
    }
    let limit: JSONValue = .object(["hours": .int(5), "scope": .null, "used": .int(91), "level": .string("near"), "headline": .bool(true)])
    let accounts = ["default", "work"].map { name -> JSONValue in
        .object(["agent": .string("claude"), "account": .string(name), "readAtMs": .int(1_792_716_922_000), "limits": .array([limit])])
    }
    try app.receive(runtimeEvent("provider.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(#require(asked)), "operation": .string("provider.usage"),
        "result": .object(["accounts": .array(accounts)]),
    ]))
    await reading.value
    let plan = LaunchPlan(launcher: app.launcher)
    #expect(plan.account?.name == "work")
    #expect(plan.usage(of: plan.account) == nil)
    #expect(plan.usage(of: plan.info?.accounts.first)?.used == 91)
}

@Test @MainActor func `a choice made here outlives an answer asked before it`() async throws {
    let (app, sent) = try launcherApp("LauncherChoice")
    defer { app.shutdownProcess() }
    var asked: String?
    for _ in 0 ..< 200 where asked == nil {
        await Task.yield()
        asked = sent().last { $0["type"] == .string("provider.agents") }?["requestId"]?.stringValue
    }
    app.launcher.choose(.tab("codex"))
    try app.receive(runtimeEvent("provider.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(#require(asked)), "operation": .string("provider.agents"), "result": catalog,
    ]))
    var change: [String: JSONValue]?
    for _ in 0 ..< 200 where app.launcher.catalog == nil || change == nil {
        await Task.yield()
        change = sent().last { $0["type"] == .string("provider.choose") }
    }
    #expect(app.launcher.catalog != nil)
    #expect(app.launcher.choice.agent == "codex")
    #expect(change?["change"] == .object(["type": .string("tab"), "agent": .string("codex")]))
}

@Test func `a model that names no efforts takes every effort`() {
    let models = AgentModels(models: [AgentModel(id: "opus", label: "Opus", efforts: []),
                                      AgentModel(id: "mini", label: "Mini", efforts: ["low"])],
                             efforts: [AgentOption(id: "low"), AgentOption(id: "high")])
    #expect(models.efforts(for: "opus").map(\.id) == ["low", "high"])
    #expect(models.efforts(for: "mini").map(\.id) == ["low"])
    #expect(models.efforts(for: nil).map(\.id) == ["low", "high"])
}

@Test func `only a local terminal at its prompt offers to start a program`() throws {
    func session(kind: String, prompt: Bool?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()), "kind": .string(kind),
            "name": .string("Terminal"), "isOwner": .bool(true), "status": .string("running"),
            "attention": .bool(false), "hidden": .bool(false), "connectionState": .string(kind == "local" ? "local" : "connected"), "sharedWith": .array([]),
        ]
        if let prompt {
            fields["prompt"] = .bool(prompt)
        }
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
    }
    #expect(try session(kind: "local", prompt: true).atPrompt)
    #expect(try !session(kind: "local", prompt: false).atPrompt)
    #expect(try !session(kind: "local", prompt: nil).atPrompt)
    #expect(try !session(kind: "remote", prompt: true).atPrompt)
}

@Test func `the command link needs no question and keeps other files`() throws {
    let home = FileManager.default.temporaryDirectory.appending(path: "kodosi-link-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: home) }
    let own = try #require(Bundle.main.executableURL?.resolvingSymlinksInPath())
    let link = home.appending(path: ".local/bin/kodosi")

    #expect(CommandLineTool.keepLinked(home: home) == "~/.local/bin/kodosi")
    #expect(link.resolvingSymlinksInPath() == own)
    #expect(CommandLineTool.keepLinked(home: home) == "~/.local/bin/kodosi")

    try FileManager.default.removeItem(at: link)
    try FileManager.default.createSymbolicLink(
        atPath: link.path, withDestinationPath: "/Applications/KodosiDesktop.app/Contents/MacOS/KodosiDesktop",
    )
    #expect(CommandLineTool.keepLinked(home: home) == "~/.local/bin/kodosi")
    #expect(link.resolvingSymlinksInPath() == own)

    try FileManager.default.removeItem(at: link)
    try Data("#!/bin/sh\n".utf8).write(to: link)
    #expect(CommandLineTool.keepLinked(home: home) == nil)
    #expect(try String(contentsOf: link, encoding: .utf8) == "#!/bin/sh\n")
}
