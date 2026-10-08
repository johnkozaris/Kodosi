import Foundation
@testable import GhosttyTerminal
import Testing

struct TerminalPasteboardContentTests {
    @Test
    func `file urls take priority over Finder display names`() {
        let text = TerminalPasteboardContent.text(
            string: "Screenshot 2026.png",
            urls: [URL(fileURLWithPath: "/Users/me/Desktop/Screenshot 2026.png")]
        )

        #expect(text == "'/Users/me/Desktop/Screenshot 2026.png'")
    }

    @Test
    func `multiple urls remain distinct shell words`() {
        let text = TerminalPasteboardContent.text(
            string: nil,
            urls: [
                URL(fileURLWithPath: "/tmp/a b"),
                URL(string: "https://example.com/path?q=1")!,
            ]
        )

        #expect(text == "'/tmp/a b' 'https://example.com/path?q=1'")
    }

    @Test
    func `plain text is the fallback`() {
        #expect(TerminalPasteboardContent.text(string: "ls -la", urls: []) == "ls -la")
        #expect(TerminalPasteboardContent.text(string: "", urls: []) == nil)
        #expect(TerminalPasteboardContent.text(string: nil, urls: []) == nil)
    }
}
