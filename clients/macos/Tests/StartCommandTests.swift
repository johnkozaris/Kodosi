import Foundation
@testable import KodosiDesktop
import Testing

@Test func aStartCommandShowsTheMarkOfItsProgram() {
    func agent(_ command: String) -> AgentKind {
        StartCommand(name: "A", command: command).agent
    }
    #expect(agent("claude") == .claude)
    #expect(agent("env CLAUDE_CONFIG_DIR=~/.claude_personal claude --continue") == .claude)
    #expect(agent("FOO=1 /opt/homebrew/bin/codex") == .codex)
    #expect(agent("cursor-agent") == .cursor)
    #expect(agent("npm run dev") == .shell)
    #expect(StartCommand(name: "A", command: "  claude  ").line == "claude")
    #expect(StartCommand(name: "A", command: " ").line == nil)
    #expect(StartCommand(name: "A", command: "claude\nrm -rf x").line == nil)
}

@Test @MainActor func anAgentThatRunsHereGetsOneStartCommand() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StartCommand"))
    let settings = DesktopSettings(defaults: defaults)
    #expect(settings.startCommands.isEmpty)
    settings.learn(.shell)
    settings.learn(.claude)
    settings.learn(.claude)
    #expect(settings.startCommands.map(\.command) == ["claude"])
    settings.startCommands.append(StartCommand(name: "Claude personal", command: "env CLAUDE_CONFIG_DIR=~/.claude_personal claude"))
    settings.startCommands.removeFirst()
    settings.saveStartCommands()

    let later = DesktopSettings(defaults: defaults)
    later.learn(.claude)
    later.learn(.cursor)
    #expect(later.startCommands.map(\.name) == ["Claude personal", "Cursor"])
    #expect(later.startCommands.last?.command == "cursor-agent")
}

@Test func onlyALocalTerminalAtItsPromptOffersToStartAProgram() throws {
    func session(kind: String, prompt: Bool?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()), "kind": .string(kind),
            "name": .string("Terminal"), "isOwner": .bool(true), "status": .string("running"),
            "connectionState": .string(kind == "local" ? "local" : "connected"), "sharedWith": .array([]),
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
