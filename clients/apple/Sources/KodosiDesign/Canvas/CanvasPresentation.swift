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

    var label: String {
        switch self {
        case .page: String(localized: "Page", bundle: .module)
        case .picture: String(localized: "Picture", bundle: .module)
        case .pdf: String(localized: "PDF", bundle: .module)
        case .office: String(localized: "Office file", bundle: .module)
        }
    }
}

public extension CanvasSection {
    var title: String {
        switch self {
        case .files: String(localized: "Files", bundle: .module)
        case .browser: String(localized: "Browser", bundle: .module)
        }
    }

    var symbol: String {
        switch self {
        case .files: "doc.on.doc"
        case .browser: "globe"
        }
    }
}

public enum CanvasSymbol {
    public static let canvas = "photo.artframe"
}

public extension BrowserDoing.Kind {
    /// What the agent does, in a few words.
    var words: String {
        switch self {
        case .open: String(localized: "Opening a page", bundle: .module)
        case .click: String(localized: "Clicking", bundle: .module)
        case .type: String(localized: "Typing", bundle: .module)
        case .scroll: String(localized: "Scrolling", bundle: .module)
        case .read: String(localized: "Reading the page", bundle: .module)
        case .wait: String(localized: "Waiting for the page", bundle: .module)
        case .other: String(localized: "Using the browser", bundle: .module)
        }
    }
}

public extension RuntimeSession {
    /// "Ask Claude Code about this", or "Ask about this" in a shell.
    var askTitle: String {
        let agent = AgentKind(program: program)
        return agent.isAgent ? String(localized: "Ask \(agent.label) about this", bundle: .module)
            : String(localized: "Ask about this", bundle: .module)
    }
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
