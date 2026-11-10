import Foundation
@testable import Kodosi
import Testing

struct TerminalAlertTests {
    @Test func `a push names its terminal and says what its agent needs`() throws {
        let asking = try #require(TerminalAlert([
            "aps": ["alert": ["loc-key": "%@ asks for permission"]],
            "kodosi": ["terminal": "t-1", "agent": "claude", "state": "blocked", "kind": "permission"],
        ]))
        #expect(asking.terminal == "t-1")
        #expect(asking.body(in: .main) == "Claude Code asks for permission")
        let waiting = try #require(TerminalAlert(["kodosi": ["terminal": "t-2", "agent": "codex", "state": "blocked"]]))
        #expect(waiting.body(in: .main) == "Codex waits for you")
        let failed = try #require(TerminalAlert(["kodosi": ["terminal": "t-3", "state": "error"]]))
        #expect(failed.body(in: .main) == "A terminal failed")
    }

    @Test func `a push without a terminal is not Kodosi's`() {
        #expect(TerminalAlert(["aps": ["alert": "Hello"]]) == nil)
        #expect(TerminalAlert(["kodosi": ["state": "done"]]) == nil)
    }
}
