@testable import KodosiDesktop
import Testing

@Test func anAccountIsNamedByWhatSetsItApart() {
    func title(_ name: String) -> String {
        AgentAccount(name: name, command: name).title(program: "claude")
    }
    #expect(title("claude") == "Default")
    #expect(title("claude-work") == "Work")
    #expect(title("claude_max") == "Max")
    #expect(title("personal") == "Personal")
    #expect(title("claude-") == "Claude-")
    #expect(title("claudex") == "Claudex")
}

@Test func accountRowsShowWhoIsSignedInAndOfferAnotherSignIn() {
    let info = AgentInfo(
        agent: "claude", program: "claude", installed: true,
        accounts: [
            AgentAccount(name: "claude", command: "claude", identity: "maya@hey.dev", folder: "~/.claude"),
            AgentAccount(name: "claude-yolo", command: "claude-yolo", identity: "maya@hey.dev", folder: "~/.claude",
                         options: "--dangerously-skip-permissions"),
            AgentAccount(name: "personal", command: "env CLAUDE_CONFIG_DIR=/h/.claude-personal claude", folder: "~/.claude-personal"),
        ],
        install: "", addsAccounts: true
    )
    let rows = LauncherMenu.accounts(info, current: "claude-yolo")
    #expect(rows.map(\.title) == ["Default", "Yolo", "Personal", "Add an account"])
    #expect(rows.map(\.current) == [false, true, false, false])
    #expect(rows[1].icon == .account(initial: "Y", key: "~/.claude", signedIn: true))
    #expect(rows[1].detail == "--dangerously-skip-permissions")
    #expect(rows[2].subtitle == "Signs in when it starts")
    #expect(rows[2].icon == .account(initial: "P", key: "~/.claude-personal", signedIn: false))
    #expect(rows[3].kind == .action)
    #expect(LauncherMenu.account.pick(rows[3].id) == .newAccount)
}
