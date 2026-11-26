import KodosiBridge
import SwiftUI

public extension CanvasShown {
    var title: String {
        switch self {
        case let .item(item): item.name
        case .browser: String(localized: "Browser", bundle: .module)
        case .screen: String(localized: "Screen", bundle: .module)
        }
    }

    var symbol: String {
        switch self {
        case let .item(item): item.kind.symbol
        case .browser: "globe"
        case .screen: CanvasSymbol.screen
        }
    }
}

public extension CanvasItem.Kind {
    var symbol: String {
        switch self {
        case .page: "doc.richtext"
        case .picture: "photo"
        case .pdf: "doc.text"
        case .document: "doc"
        case .sheet: "tablecells"
        case .presentation: "play.rectangle"
        }
    }

    var label: String {
        switch self {
        case .page: String(localized: "Page", bundle: .module)
        case .picture: String(localized: "Picture", bundle: .module)
        case .pdf: String(localized: "PDF", bundle: .module)
        case .document: String(localized: "Document", bundle: .module)
        case .sheet: String(localized: "Sheet", bundle: .module)
        case .presentation: String(localized: "Presentation", bundle: .module)
        }
    }
}

public extension CanvasItem {
    var label: String {
        view == .text ? String(localized: "Text", bundle: .module) : kind.label
    }
}

public extension CanvasSection {
    var title: String {
        switch self {
        case .files: String(localized: "Files", bundle: .module)
        case .browser: String(localized: "Browser", bundle: .module)
        case .screen: String(localized: "Screen", bundle: .module)
        }
    }

    var symbol: String {
        switch self {
        case .files: "doc.on.doc"
        case .browser: "globe"
        case .screen: CanvasSymbol.screen
        }
    }
}

public enum CanvasSymbol {
    public static let canvas = "photo.artframe"
    public static let addFile = "doc.badge.plus"
    public static let screen = "display"
}

public extension RuntimeSession {
    var askTitle: String {
        let agent = AgentKind(program: program)
        return agent.isAgent ? String(localized: "Ask \(agent.label) about this", bundle: .module)
            : String(localized: "Ask about this", bundle: .module)
    }
}

public enum CanvasQuestion {
    public static func about(_ shown: CanvasShown, words: String?) -> String {
        let place = switch shown {
        case let .item(item): writtenOut(item.key)
        case let .browser(browser): (browser.activeTab?.url).map(writtenOut) ?? String(localized: "the shared browser", bundle: .module)
        case .screen: String(localized: "the screen of this computer", bundle: .module)
        }
        let words = words.map(oneLine).flatMap { $0.isEmpty ? nil : String($0.prefix(600)) }
        return words.map { String(localized: "About \(place): \"\($0)\" ", bundle: .module) }
            ?? String(localized: "About \(place): ", bundle: .module)
    }

    /// A name of a file can hold a line break, which would end the line that the agent reads.
    private static func writtenOut(_ text: String) -> String {
        text.unicodeScalars.map { scalar in
            CharacterSet.controlCharacters.contains(scalar) || CharacterSet.newlines.contains(scalar)
                ? "\\u{\(String(scalar.value, radix: 16, uppercase: true))}" : String(scalar)
        }.joined()
    }

    private static func oneLine(_ text: String) -> String {
        text.unicodeScalars.map { CharacterSet.controlCharacters.contains($0) || CharacterSet.newlines.contains($0) ? " " : String($0) }
            .joined()
            .split(separator: " ", omittingEmptySubsequences: true)
            .joined(separator: " ")
    }
}
