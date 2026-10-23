@testable import KodosiDesktop
import Testing

@Test func `an account is named by what sets it apart`() {
    func title(_ name: String) -> String {
        AgentAccount(name: name, command: name).title(program: "claude")
    }
    #expect(title("claude") == "Default")
    #expect(title("default") == "Default")
    #expect(title("claude-work") == "Work")
    #expect(title("claude_max") == "Max")
    #expect(title("personal") == "Personal")
    #expect(title("claude-") == "Claude-")
    #expect(title("claudex") == "Claudex")
}

@Test func `account rows show who is signed in and offer another sign in`() {
    let info = AgentInfo(
        agent: "claude", program: "claude", installed: true,
        accounts: [
            AgentAccount(name: "default", command: "claude", identity: "maya@hey.dev", folder: "~/.claude"),
            AgentAccount(name: "work", command: "env CLAUDE_CONFIG_DIR=/h/.claude-work claude", identity: "maya@acme.studio",
                         folder: "~/.claude-work"),
            AgentAccount(name: "personal", command: "env CLAUDE_CONFIG_DIR=/h/.claude-personal claude", folder: "~/.claude-personal"),
        ],
        install: "", addsAccounts: true, reportsUsage: true,
    )
    let rows = LauncherMenu.accounts(info, current: "work")
    #expect(rows.map(\.title) == ["Default", "Work", "Personal", "Add an account"])
    #expect(rows.map(\.current) == [false, true, false, false])
    #expect(rows[1].subtitle == "maya@acme.studio")
    #expect(rows[1].icon == .account(initial: "W", key: "~/.claude-work", signedIn: true))
    #expect(rows[2].subtitle == "Signs in when it starts")
    #expect(rows[2].icon == .account(initial: "P", key: "~/.claude-personal", signedIn: false))
    #expect(LauncherMenu.account.pick(rows[1].id) == .account("work"))
    #expect(rows[3].kind == .action)
    #expect(LauncherMenu.account.pick(rows[3].id) == .newAccount)
}
