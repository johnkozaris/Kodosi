import KodosiBridge
import SwiftUI

public extension CanvasShown {
    var title: String {
        switch self {
        case let .item(item): item.name
        case .browser: String(localized: "Browser", bundle: .module)
        }
    }

    var symbol: String {
        switch self {
        case let .item(item): item.kind.symbol
        case .browser: "globe"
        }
    }

    var missing: Bool {
        if case let .item(item) = self {
            return item.missing
        }
        return false
    }
}

public extension CanvasItem.Kind {
    var symbol: String {
        switch self {
        case .page: "doc.richtext"
        case .picture: "photo"
        case .pdf: "doc.text"
        case .office: "doc"
        }
    }
}

public enum CanvasSymbol {
    public static let canvas = "photo.artframe"
}

/// The words that "Ask about this" types into the terminal: one line, which the person
/// finishes with a question.
public enum CanvasQuestion {
    public static func about(_ shown: CanvasShown, words: String?) -> String {
        let place = switch shown {
        case let .item(item): item.key
        case let .browser(browser): browser.activeTab?.url ?? String(localized: "the shared browser", bundle: .module)
        }
        let words = words.map(oneLine).flatMap { $0.isEmpty ? nil : String($0.prefix(600)) }
        return words.map { "About \(place): \"\($0)\" " } ?? "About \(place): "
    }

    private static func oneLine(_ text: String) -> String {
        text.unicodeScalars.map { CharacterSet.controlCharacters.contains($0) || CharacterSet.newlines.contains($0) ? " " : String($0) }
            .joined()
            .split(separator: " ", omittingEmptySubsequences: true)
            .joined(separator: " ")
    }
}
