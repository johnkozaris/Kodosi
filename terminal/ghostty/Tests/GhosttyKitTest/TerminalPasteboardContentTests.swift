import AppKit
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
    func `program clipboard writes keep every representation`() {
        let pasteboard = NSPasteboard.withUniqueName()
        defer { pasteboard.releaseGlobally() }
        let image = Data([0x89, 0x50, 0x4E, 0x47])

        TerminalPasteboardContent.write(
            [
                .init(mime: "text/plain", data: Data("copied by a program".utf8)),
                .init(mime: "text/html", data: Data("<b>copied</b>".utf8)),
                .init(mime: "image/png", data: image),
            ],
            to: pasteboard
        )

        #expect(pasteboard.string(forType: .string) == "copied by a program")
        #expect(pasteboard.string(forType: .html) == "<b>copied</b>")
        #expect(pasteboard.data(forType: .png) == image)
    }

    @Test
    func `plain text is the fallback`() {
        #expect(TerminalPasteboardContent.text(string: "ls -la", urls: []) == "ls -la")
        #expect(TerminalPasteboardContent.text(string: "", urls: []) == nil)
        #expect(TerminalPasteboardContent.text(string: nil, urls: []) == nil)
    }
}
