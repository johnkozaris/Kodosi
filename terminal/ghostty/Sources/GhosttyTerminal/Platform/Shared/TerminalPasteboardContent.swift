import Foundation
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

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

    #if os(macOS)
    static func text(from pasteboard: NSPasteboard = .general) -> String? {
        text(
            string: pasteboard.string(forType: .string),
            urls: (pasteboard.readObjects(forClasses: [NSURL.self]) as? [URL]) ?? []
        )
    }

    static func plainText(from pasteboard: NSPasteboard = .general) -> String? {
        pasteboard.string(forType: .string)
    }

    static func write(_ string: String, to pasteboard: NSPasteboard = .general) {
        pasteboard.clearContents()
        pasteboard.setString(string, forType: .string)
    }
    #elseif os(iOS)
    static func text(from pasteboard: UIPasteboard = .general) -> String? {
        text(string: pasteboard.string, urls: pasteboard.urls ?? [])
    }

    static func plainText(from pasteboard: UIPasteboard = .general) -> String? {
        pasteboard.hasStrings ? pasteboard.string : nil
    }

    static func write(_ string: String, to pasteboard: UIPasteboard = .general) {
        pasteboard.string = string
    }
    #endif
}
