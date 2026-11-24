import KodosiBridge
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
