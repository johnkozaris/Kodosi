import Foundation
import KodosiBridge

public enum DosiWords {
    public struct Phrase: Sendable {
        let alone: String
        let about: @Sendable (String) -> String

        func callAsFunction(_ place: String?) -> String {
            place.map(about) ?? alone
        }
    }

    public struct Deed: Sendable {
        let done: Phrase
        let doing: Phrase
        let symbol: String
        var said: Phrase?
        var saying: Phrase?
    }

    public static let deeds: [DosiTool.Action: Deed] = [
        .look: Deed(
            done: Phrase(alone: String(localized: "looked at", bundle: .module)) { String(localized: "looked at \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "looking at", bundle: .module)) { String(localized: "looking at \($0)", bundle: .module) },
            symbol: "eye",
        ),
        .read: Deed(
            done: Phrase(alone: String(localized: "read", bundle: .module)) { String(localized: "read \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "reading", bundle: .module)) { String(localized: "reading \($0)", bundle: .module) },
            symbol: "text.alignleft",
        ),
        .type: Deed(
            done: Phrase(alone: String(localized: "typed in", bundle: .module)) { String(localized: "typed in \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "typing in", bundle: .module)) { String(localized: "typing in \($0)", bundle: .module) },
            symbol: "keyboard",
            said: Phrase(alone: String(localized: "typed", bundle: .module)) { String(localized: "typed \($0)", bundle: .module) },
            saying: Phrase(alone: String(localized: "typing", bundle: .module)) { String(localized: "typing \($0)", bundle: .module) },
        ),
        .press: Deed(
            done: Phrase(alone: String(localized: "pressed keys in", bundle: .module)) { String(localized: "pressed keys in \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "pressing keys in", bundle: .module)) { String(localized: "pressing keys in \($0)", bundle: .module) },
            symbol: "command",
            said: Phrase(alone: String(localized: "pressed", bundle: .module)) { String(localized: "pressed \($0)", bundle: .module) },
            saying: Phrase(alone: String(localized: "pressing", bundle: .module)) { String(localized: "pressing \($0)", bundle: .module) },
        ),
        .start: Deed(
            done: Phrase(alone: String(localized: "started", bundle: .module)) { String(localized: "started \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "starting", bundle: .module)) { String(localized: "starting \($0)", bundle: .module) },
            symbol: "play.fill",
        ),
        .interrupt: Deed(
            done: Phrase(alone: String(localized: "interrupted", bundle: .module)) { String(localized: "interrupted \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "interrupting", bundle: .module)) { String(localized: "interrupting \($0)", bundle: .module) },
            symbol: "stop.fill",
        ),
        .close: Deed(
            done: Phrase(alone: String(localized: "closed", bundle: .module)) { String(localized: "closed \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "closing", bundle: .module)) { String(localized: "closing \($0)", bundle: .module) },
            symbol: "xmark",
        ),
        .show: Deed(
            done: Phrase(alone: String(localized: "showed", bundle: .module)) { String(localized: "showed \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "showing", bundle: .module)) { String(localized: "showing \($0)", bundle: .module) },
            symbol: "rectangle.on.rectangle",
        ),
        .wait: Deed(
            done: Phrase(alone: String(localized: "waited for", bundle: .module)) { String(localized: "waited for \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "waiting for", bundle: .module)) { String(localized: "waiting for \($0)", bundle: .module) },
            symbol: "hourglass",
        ),
        .post: Deed(
            done: Phrase(alone: String(localized: "posted in", bundle: .module)) { String(localized: "posted in \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "posting in", bundle: .module)) { String(localized: "posting in \($0)", bundle: .module) },
            symbol: "bubble.left.fill",
            said: Phrase(alone: String(localized: "posted", bundle: .module)) { String(localized: "posted \($0)", bundle: .module) },
            saying: Phrase(alone: String(localized: "posting", bundle: .module)) { String(localized: "posting \($0)", bundle: .module) },
        ),
        .run: Deed(
            done: Phrase(alone: String(localized: "ran", bundle: .module)) { String(localized: "ran \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "running", bundle: .module)) { String(localized: "running \($0)", bundle: .module) },
            symbol: "chevron.right",
        ),
        .write: Deed(
            done: Phrase(alone: String(localized: "wrote", bundle: .module)) { String(localized: "wrote \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "writing", bundle: .module)) { String(localized: "writing \($0)", bundle: .module) },
            symbol: "square.and.pencil",
        ),
        .edit: Deed(
            done: Phrase(alone: String(localized: "changed", bundle: .module)) { String(localized: "changed \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "changing", bundle: .module)) { String(localized: "changing \($0)", bundle: .module) },
            symbol: "pencil",
        ),
        .search: Deed(
            done: Phrase(alone: String(localized: "searched for", bundle: .module)) { String(localized: "searched for \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "searching for", bundle: .module)) { String(localized: "searching for \($0)", bundle: .module) },
            symbol: "magnifyingglass",
        ),
        .fetch: Deed(
            done: Phrase(alone: String(localized: "fetched", bundle: .module)) { String(localized: "fetched \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "fetching", bundle: .module)) { String(localized: "fetching \($0)", bundle: .module) },
            symbol: "globe",
        ),
        .plan: Deed(
            done: Phrase(alone: String(localized: "planned", bundle: .module)) { String(localized: "planned \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "planning", bundle: .module)) { String(localized: "planning \($0)", bundle: .module) },
            symbol: "checklist",
        ),
        .delegate: Deed(
            done: Phrase(alone: String(localized: "asked a helper", bundle: .module)) { String(localized: "asked a helper about \($0)", bundle: .module) },
            doing: Phrase(alone: String(localized: "asking a helper", bundle: .module)) { String(localized: "asking a helper about \($0)", bundle: .module) },
            symbol: "person.2",
        ),
        .other: other,
    ]

    private static let other = Deed(
        done: Phrase(alone: String(localized: "used", bundle: .module)) { String(localized: "used \($0)", bundle: .module) },
        doing: Phrase(alone: String(localized: "using", bundle: .module)) { String(localized: "using \($0)", bundle: .module) },
        symbol: "wrench.and.screwdriver",
    )

    private static let workedOn = Phrase(alone: String(localized: "worked on", bundle: .module)) { String(localized: "worked on \($0)", bundle: .module) }
    private static let workingOn = Phrase(alone: String(localized: "working on", bundle: .module)) { String(localized: "working on \($0)", bundle: .module) }

    static func deed(_ action: DosiTool.Action) -> Deed {
        deeds[action] ?? other
    }

    static func target(_ target: DosiTarget) -> String {
        switch target.kind {
        case .terminals: String(localized: "the terminals", bundle: .module)
        case .rooms: String(localized: "the projects", bundle: .module)
        default: target.value
        }
    }

    public static func receipt(_ tool: DosiTool) -> String {
        let deed = deed(tool.action)
        let place = tool.target.map(target).flatMap { $0.isEmpty ? nil : $0 }
        guard tool.text?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else {
            return tool.status == .running ? deed.doing(place) : deed.done(place)
        }
        let said = said(tool)
        return place.map { "\(said) · \($0)" } ?? said
    }

    public static func said(_ tool: DosiTool) -> String {
        let deed = deed(tool.action)
        let phrase = tool.status == .running ? deed.saying ?? deed.doing : deed.said ?? deed.done
        let line = tool.text?.trimmingCharacters(in: .whitespacesAndNewlines)
            .split(separator: "\n").first.map(String.init)
        return phrase(line)
    }

    public static func summary(_ tools: [DosiTool]) -> String {
        let running = tools.contains { $0.status == .running }
        let actions = Set(tools.map(\.action))
        let phrase = if actions.count == 1, let action = actions.first {
            running ? deed(action).doing : deed(action).done
        } else {
            running ? workingOn : workedOn
        }
        var names: [String] = []
        for name in tools.compactMap({ $0.target.map(target) }) where !name.isEmpty && !names.contains(name) {
            names.append(name)
        }
        guard !names.isEmpty else { return phrase(nil) }
        let shown = phrase(names.prefix(2).joined(separator: ", "))
        return names.count > 2 ? "\(shown) +\(names.count - 2)" : shown
    }
}
