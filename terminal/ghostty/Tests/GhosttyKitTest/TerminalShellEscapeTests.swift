@testable import GhosttyTerminal
import Testing

struct TerminalShellEscapeTests {
    @Test
    func `plain paths become complete shell words`() {
        #expect(
            TerminalShellEscape.escape("/tmp/ghostty/image.png")
                == "'/tmp/ghostty/image.png'"
        )
    }

    @Test
    func `shell metacharacters stay inside one quoted word`() {
        #expect(
            TerminalShellEscape.escape("/Users/me/My File (1).png")
                == "'/Users/me/My File (1).png'"
        )
        #expect(TerminalShellEscape.escape("a'b\"c$d`e") == "'a'\\''b\"c$d`e'")
        #expect(TerminalShellEscape.escape("x\\y") == "'x\\y'")
    }

    @Test
    func `unicode and newlines remain literal path content`() {
        #expect(TerminalShellEscape.escape("/tmp/截图 1.png") == "'/tmp/截图 1.png'")
        #expect(TerminalShellEscape.escape("/tmp/first\nsecond") == "'/tmp/first\nsecond'")
    }
}
