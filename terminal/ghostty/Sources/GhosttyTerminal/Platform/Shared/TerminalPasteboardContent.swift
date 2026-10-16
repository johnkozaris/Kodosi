import Foundation
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

enum TerminalPasteboardContent {
    struct Representation: Equatable {
        let mime: String
        let data: Data
    }

    static func typeIdentifier(mime: String) -> String {
        if mime.hasPrefix("text/plain") {
            return UTType.utf8PlainText.identifier
        }
        return UTType(mimeType: mime)?.identifier ?? mime
    }

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

    static func write(_ representations: [Representation], to pasteboard: NSPasteboard = .general) {
        let types = representations.map {
            NSPasteboard.PasteboardType(typeIdentifier(mime: $0.mime))
        }
        pasteboard.declareTypes(types, owner: nil)
        for (type, representation) in zip(types, representations) {
            pasteboard.setData(representation.data, forType: type)
        }
    }
    #elseif os(iOS)
    static func text(from pasteboard: UIPasteboard = .general) -> String? {
        text(string: pasteboard.string, urls: pasteboard.urls ?? [])
    }

    static func write(_ representations: [Representation], to pasteboard: UIPasteboard = .general) {
        pasteboard.setItems([Dictionary(
            representations.map { (typeIdentifier(mime: $0.mime), $0.data as Any) },
            uniquingKeysWith: { _, last in last }
        )])
    }
    #endif
}
