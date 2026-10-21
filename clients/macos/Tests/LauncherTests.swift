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
            "addsAccounts": .bool(true),
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

@Test @MainActor func newTerminalFocusesTheHomeLauncherAndOpensASheetElsewhere() throws {
    let (app, _) = try launcherApp("LauncherHome")
    defer { app.shutdownProcess() }
    app.openLauncher()
    #expect(app.workbench.launcher == nil)
    #expect(app.workbench.launcherFocus == 1)
    app.openLauncher(folder: "/work/atlas")
    #expect(app.workbench.launcher == nil)
    #expect(app.workbench.launcherDraft.folder == "/work/atlas")
    app.openLauncher(roomId: "room")
    #expect(app.workbench.launcher == LauncherRequest(roomId: "room"))
    app.workbench.launcherDraft.prompt = "fix the build"
    app.workbench.launcher = nil
    app.workbench.section = .people
    app.openLauncher()
    #expect(app.workbench.launcher == LauncherRequest())
    #expect(app.workbench.launcherDraft == LauncherDraft(prompt: "fix the build", folder: "/work/atlas"))
    app.workbench.switchAccount("another-user")
    #expect(app.workbench.launcherDraft == LauncherDraft())
}

@Test @MainActor func aTerminalStartedForARoomJoinsThatRoom() async throws {
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

@Test @MainActor func theLauncherStartsTheAgentAsItShowsIt() async throws {
    let (app, sent) = try launcherApp("LauncherPlan")
    defer { app.shutdownProcess() }
    try await answerAgentRequests(app, sent: sent)
    #expect(app.launcher.installed.map(\.agent) == ["claude"])
    #expect(LaunchPlan(launcher: app.launcher).start == AgentStart(agent: "claude", account: "work", model: "", effort: "high"))
    #expect(LaunchPlan(launcher: app.launcher, agent: "codex").start == nil)
    app.launcher.choose(.pick(AgentPick(agent: "claude", account: "gone")))
    #expect(LaunchPlan(launcher: app.launcher).account?.name == "default")
}

@Test @MainActor func aChoiceMadeHereOutlivesAnAnswerAskedBeforeIt() async throws {
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

@Test func aModelThatNamesNoEffortsTakesEveryEffort() {
    let models = AgentModels(models: [AgentModel(id: "opus", label: "Opus", efforts: []),
                                      AgentModel(id: "mini", label: "Mini", efforts: ["low"])],
                             efforts: [AgentOption(id: "low"), AgentOption(id: "high")])
    #expect(models.efforts(for: "opus").map(\.id) == ["low", "high"])
    #expect(models.efforts(for: "mini").map(\.id) == ["low"])
    #expect(models.efforts(for: nil).map(\.id) == ["low", "high"])
}

@Test func onlyALocalTerminalAtItsPromptOffersToStartAProgram() throws {
    func session(kind: String, prompt: Bool?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()), "kind": .string(kind),
            "name": .string("Terminal"), "isOwner": .bool(true), "status": .string("running"),
            "attention": .bool(false), "connectionState": .string(kind == "local" ? "local" : "connected"), "sharedWith": .array([]),
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

@Test func theCommandLinkNeedsNoQuestionAndKeepsOtherFiles() throws {
    let home = FileManager.default.temporaryDirectory.appending(path: "kodosi-link-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: home) }
    let own = try #require(Bundle.main.executableURL?.resolvingSymlinksInPath())
    let link = home.appending(path: ".local/bin/kodosi")

    #expect(CommandLineTool.keepLinked(home: home) == "~/.local/bin/kodosi")
    #expect(link.resolvingSymlinksInPath() == own)
    #expect(CommandLineTool.keepLinked(home: home) == "~/.local/bin/kodosi")

    try FileManager.default.removeItem(at: link)
    try FileManager.default.createSymbolicLink(
        atPath: link.path, withDestinationPath: "/Applications/KodosiDesktop.app/Contents/MacOS/KodosiDesktop"
    )
    #expect(CommandLineTool.keepLinked(home: home) == "~/.local/bin/kodosi")
    #expect(link.resolvingSymlinksInPath() == own)

    try FileManager.default.removeItem(at: link)
    try Data("#!/bin/sh\n".utf8).write(to: link)
    #expect(CommandLineTool.keepLinked(home: home) == nil)
    #expect(try String(contentsOf: link, encoding: .utf8) == "#!/bin/sh\n")
}
