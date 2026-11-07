import Foundation
import KodosiBridge

enum DosiWords {
    struct Phrase: Sendable {
        let alone: String
        let about: @Sendable (String) -> String

        func callAsFunction(_ place: String?) -> String {
            place.map(about) ?? alone
        }
    }

    struct Deed: Sendable {
        let done: Phrase
        let doing: Phrase
        let symbol: String
        var said: Phrase?
        var saying: Phrase?
    }

    static let deeds: [DosiTool.Action: Deed] = [
        .look: Deed(
            done: Phrase(alone: String(localized: "looked at")) { String(localized: "looked at \($0)") },
            doing: Phrase(alone: String(localized: "looking at")) { String(localized: "looking at \($0)") },
            symbol: "eye",
        ),
        .read: Deed(
            done: Phrase(alone: String(localized: "read")) { String(localized: "read \($0)") },
            doing: Phrase(alone: String(localized: "reading")) { String(localized: "reading \($0)") },
            symbol: "text.alignleft",
        ),
        .type: Deed(
            done: Phrase(alone: String(localized: "typed in")) { String(localized: "typed in \($0)") },
            doing: Phrase(alone: String(localized: "typing in")) { String(localized: "typing in \($0)") },
            symbol: "keyboard",
            said: Phrase(alone: String(localized: "typed")) { String(localized: "typed \($0)") },
            saying: Phrase(alone: String(localized: "typing")) { String(localized: "typing \($0)") },
        ),
        .press: Deed(
            done: Phrase(alone: String(localized: "pressed keys in")) { String(localized: "pressed keys in \($0)") },
            doing: Phrase(alone: String(localized: "pressing keys in")) { String(localized: "pressing keys in \($0)") },
            symbol: "command",
            said: Phrase(alone: String(localized: "pressed")) { String(localized: "pressed \($0)") },
            saying: Phrase(alone: String(localized: "pressing")) { String(localized: "pressing \($0)") },
        ),
        .start: Deed(
            done: Phrase(alone: String(localized: "started")) { String(localized: "started \($0)") },
            doing: Phrase(alone: String(localized: "starting")) { String(localized: "starting \($0)") },
            symbol: "play.fill",
        ),
        .interrupt: Deed(
            done: Phrase(alone: String(localized: "interrupted")) { String(localized: "interrupted \($0)") },
            doing: Phrase(alone: String(localized: "interrupting")) { String(localized: "interrupting \($0)") },
            symbol: "stop.fill",
        ),
        .close: Deed(
            done: Phrase(alone: String(localized: "closed")) { String(localized: "closed \($0)") },
            doing: Phrase(alone: String(localized: "closing")) { String(localized: "closing \($0)") },
            symbol: "xmark",
        ),
        .show: Deed(
            done: Phrase(alone: String(localized: "showed")) { String(localized: "showed \($0)") },
            doing: Phrase(alone: String(localized: "showing")) { String(localized: "showing \($0)") },
            symbol: "rectangle.on.rectangle",
        ),
        .wait: Deed(
            done: Phrase(alone: String(localized: "waited for")) { String(localized: "waited for \($0)") },
            doing: Phrase(alone: String(localized: "waiting for")) { String(localized: "waiting for \($0)") },
            symbol: "hourglass",
        ),
        .post: Deed(
            done: Phrase(alone: String(localized: "posted in")) { String(localized: "posted in \($0)") },
            doing: Phrase(alone: String(localized: "posting in")) { String(localized: "posting in \($0)") },
            symbol: "bubble.left.fill",
            said: Phrase(alone: String(localized: "posted")) { String(localized: "posted \($0)") },
            saying: Phrase(alone: String(localized: "posting")) { String(localized: "posting \($0)") },
        ),
        .run: Deed(
            done: Phrase(alone: String(localized: "ran")) { String(localized: "ran \($0)") },
            doing: Phrase(alone: String(localized: "running")) { String(localized: "running \($0)") },
            symbol: "chevron.right",
        ),
        .write: Deed(
            done: Phrase(alone: String(localized: "wrote")) { String(localized: "wrote \($0)") },
            doing: Phrase(alone: String(localized: "writing")) { String(localized: "writing \($0)") },
            symbol: "square.and.pencil",
        ),
        .edit: Deed(
            done: Phrase(alone: String(localized: "changed")) { String(localized: "changed \($0)") },
            doing: Phrase(alone: String(localized: "changing")) { String(localized: "changing \($0)") },
            symbol: "pencil",
        ),
        .search: Deed(
            done: Phrase(alone: String(localized: "searched for")) { String(localized: "searched for \($0)") },
            doing: Phrase(alone: String(localized: "searching for")) { String(localized: "searching for \($0)") },
            symbol: "magnifyingglass",
        ),
        .fetch: Deed(
            done: Phrase(alone: String(localized: "fetched")) { String(localized: "fetched \($0)") },
            doing: Phrase(alone: String(localized: "fetching")) { String(localized: "fetching \($0)") },
            symbol: "globe",
        ),
        .plan: Deed(
            done: Phrase(alone: String(localized: "planned")) { String(localized: "planned \($0)") },
            doing: Phrase(alone: String(localized: "planning")) { String(localized: "planning \($0)") },
            symbol: "checklist",
        ),
        .delegate: Deed(
            done: Phrase(alone: String(localized: "asked a helper")) { String(localized: "asked a helper about \($0)") },
            doing: Phrase(alone: String(localized: "asking a helper")) { String(localized: "asking a helper about \($0)") },
            symbol: "person.2",
        ),
        .other: other,
    ]

    private static let other = Deed(
        done: Phrase(alone: String(localized: "used")) { String(localized: "used \($0)") },
        doing: Phrase(alone: String(localized: "using")) { String(localized: "using \($0)") },
        symbol: "wrench.and.screwdriver",
    )

    private static let workedOn = Phrase(alone: String(localized: "worked on")) { String(localized: "worked on \($0)") }
    private static let workingOn = Phrase(alone: String(localized: "working on")) { String(localized: "working on \($0)") }

    static func deed(_ action: DosiTool.Action) -> Deed {
        deeds[action] ?? other
    }

    static func target(_ target: DosiTarget) -> String {
        switch target.kind {
        case .terminals: String(localized: "the terminals")
        case .rooms: String(localized: "the rooms")
        default: target.value
        }
    }

    static func receipt(_ tool: DosiTool) -> String {
        let deed = deed(tool.action)
        let place = tool.target.map(target).flatMap { $0.isEmpty ? nil : $0 }
        guard tool.text?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else {
            return tool.status == .running ? deed.doing(place) : deed.done(place)
        }
        let said = said(tool)
        return place.map { "\(said) · \($0)" } ?? said
    }

    static func said(_ tool: DosiTool) -> String {
        let deed = deed(tool.action)
        let phrase = tool.status == .running ? deed.saying ?? deed.doing : deed.said ?? deed.done
        let line = tool.text?.trimmingCharacters(in: .whitespacesAndNewlines)
            .split(separator: "\n").first.map(String.init)
        return phrase(line)
    }

    static func summary(_ tools: [DosiTool]) -> String {
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
