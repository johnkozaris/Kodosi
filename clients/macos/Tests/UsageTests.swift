import Foundation
@testable import KodosiDesktop
import Testing

@Test func `limits are named by their window and spoken in full`() throws {
    let answer = """
    {"accounts": [
      {"agent": "claude", "account": "work", "readAt": 1792716922, "limits": [
        {"hours": 5, "scope": null, "used": 42, "level": "normal", "headline": false},
        {"hours": 168, "scope": null, "used": 85, "level": "near", "headline": true},
        {"hours": 168, "scope": "Fable", "used": 100, "level": "reached", "headline": false}]},
      {"agent": "copilot", "account": "default", "readAt": 1792716922, "limits": [
        {"hours": null, "scope": null, "used": 59, "level": "normal", "headline": true}]}]}
    """
    let reply = try JSONDecoder().decode(AgentUsageReply.self, from: Data(answer.utf8))
    let claude = try #require(reply.accounts.first)
    #expect(claude.limits.map(\.window) == ["5h", "week", "Fable"])
    #expect(claude.limits.map(\.id) == ["hours:5", "hours:168", "scope:Fable"])
    #expect(claude.headline?.used == 85)
    #expect(claude.limits.map(\.spoken) == [
        "Last 5 hours: 42% used", "This week: 85% used", "Fable, this week: 100% used",
    ])
    let copilot = try #require(reply.accounts.last?.limits.first)
    #expect(copilot.window == "month")
    #expect(copilot.spoken == "This month: 59% used")
}

@Test func `account rows carry the headline limit of their account`() {
    let info = AgentInfo(
        agent: "claude", program: "claude", installed: true,
        accounts: [
            AgentAccount(name: "default", command: "claude", identity: "maya@hey.dev", folder: "~/.claude"),
            AgentAccount(name: "work", command: "env CLAUDE_CONFIG_DIR=/h/.claude-work claude", identity: "maya@acme.studio",
                         folder: "~/.claude-work"),
        ],
        install: "", addsAccounts: true, reportsUsage: true,
    )
    let limit = UsageLimit(hours: 5, scope: nil, used: 91, level: .near, headline: true)
    let rows = LauncherMenu.accounts(info, current: "work") { $0.name == "work" ? limit : nil }
    #expect(rows.map(\.usage) == [nil, limit, nil])
}
