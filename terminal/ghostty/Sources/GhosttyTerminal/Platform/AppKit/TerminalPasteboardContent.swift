import AppKit
import Foundation

enum TerminalPasteboardContent {
    static func text(string: String?, urls: [URL]) -> String? {
        if !urls.isEmpty {
            return urls
                .map {
                    TerminalShellEscape.escape(
                        $0.isFileURL ? $0.path : $0.absoluteString
                    )
                }
                .joined(separator: " ")
        }
        guard let string, !string.isEmpty else { return nil }
        return string
    }

    static func text(from pasteboard: NSPasteboard = .general) -> String? {
        text(
            string: pasteboard.string(forType: .string),
            urls: (pasteboard.readObjects(forClasses: [NSURL.self]) as? [URL]) ?? []
        )
    }
}
