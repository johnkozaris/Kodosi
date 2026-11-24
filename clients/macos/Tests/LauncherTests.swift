import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

private func launcherApp(_ prefix: String) throws -> (AppDependencies, () -> [[String: JSONValue]]) {
    let defaults = try #require(EphemeralUserDefaults(prefix: prefix))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeReady(epoch: 0, userId: nil))
    return (app, { sent })
}

private let catalog: JSONValue = .object([
    "kodosi": .null,
    "choice": .object([
        "agent": .string("claude"),
        "picks": .array([
            .object(["agent": .string("claude"), "account": .string("work"), "model": .null, "effort": .string("high"), "fast": .bool(true),
                     "chat": .bool(false)]),
        ]),
    ]),
    "agents": .array([
        .object([
            "agent": .string("claude"), "program": .string("claude"), "installed": .bool(true), "install": .string(""),
            "addsAccounts": .bool(true), "reportsUsage": .bool(true),
            "permissions": .array(["ask", "auto", "unrestricted"].map(JSONValue.string)),
            "outputs": .array(["quiet", "standard", "detailed"].map(JSONValue.string)),
            "defaultPermissions": .string("unrestricted"), "defaultOutput": .string("quiet"), "fast": .bool(true), "chat": .bool(false),
            "dosi": .bool(true), "accounts": .array([
                .object(["name": .string("default"), "command": .string("claude"), "identity": .string("ada@example.com"),
                         "folder": .string("~/.claude")]),
                .object(["name": .string("work"), "command": .string("env CLAUDE_CONFIG_DIR=/Users/ada/.claude-work claude"),
                         "identity": .null, "folder": .string("~/.claude-work")]),
            ]),
        ]),
        .object([
            "agent": .string("codex"), "program": .string("codex"), "installed": .bool(false),
            "install": .string("curl -fsSL https://chatgpt.com/codex/install.sh | sh"), "addsAccounts": .bool(true),
            "reportsUsage": .bool(true), "permissions": .array(["ask", "auto", "unrestricted"].map(JSONValue.string)),
            "outputs": .array(["quiet", "standard", "detailed"].map(JSONValue.string)),
            "defaultPermissions": .string("unrestricted"), "defaultOutput": .string("quiet"), "fast": .bool(true), "chat": .bool(false),
            "dosi": .bool(true), "accounts": .array([]),
        ]),
    ]),
])

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

@Test func `a quick terminal is a plain terminal and skips the launcher`() async throws {
    let (app, sent) = try launcherApp("QuickPlain")
    defer { app.shutdownProcess() }
    app.workbench.launcher = LauncherRequest()
    app.openQuickTerminal()
    app.openQuickTerminal()
    var creates: [[String: JSONValue]] = []
    for _ in 0 ..< 200 where creates.isEmpty {
        await Task.yield()
        creates = sent().filter { $0["type"] == .string("session.create") }
    }
    let fields = try #require(creates.first)
    #expect(creates.count == 1)
    #expect(fields["agent"] == nil)
    #expect(fields["command"] == nil)
    #expect(fields["computer"] == nil)
    #expect(app.workbench.launcher == nil)
    #expect(app.typeAhead.isHolding)
    let created = UUIDv7.generate()
    try app.receive(runtimeEvent("session.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(#require(fields["requestId"]?.stringValue)), "sessionId": .string(created),
        "operation": .string("session.create"),
    ]))
    for _ in 0 ..< 200 where !app.freshTerminals.contains(created) {
        await Task.yield()
    }
    app.typeAhead.deliver(to: UUIDv7.generate())
    #expect(app.typeAhead.isHolding)
    app.typeAhead.deliver(to: created)
    #expect(!app.typeAhead.isHolding)
}

@Test func `the launcher starts the agent as it shows it`() async throws {
    let (app, sent) = try launcherApp("LauncherPlan")
    defer { app.shutdownProcess() }
    try await answerAgentRequests(app, sent: sent)
    #expect(app.launcher.installed.map(\.agent) == ["claude"])
    #expect(LaunchPlan(launcher: app.launcher).start == AgentStart(
        agent: "claude", account: "work", model: "", effort: "high", permissions: .unrestricted, output: .quiet, fast: true,
    ))
    #expect(LaunchPlan(launcher: app.launcher, agent: "codex").start == nil)
    app.launcher.choose(.pick(AgentPick(agent: "claude", account: "gone", permissions: .ask, output: .standard)))
    let plan = LaunchPlan(launcher: app.launcher)
    #expect(plan.account?.name == "default")
    #expect(plan.start?.permissions == .ask && plan.start?.output == .standard && plan.start?.fast == false)
    #expect(plan.shown?.permissions == .ask)
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
