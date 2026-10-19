import Foundation
import KodosiBridge

enum DosiWords {
    struct Deed: Equatable {
        let done: String
        let doing: String
        let symbol: String
        var saying: String?
        var said: String?
    }

    static let deeds: [DosiTool.Action: Deed] = [
        .look: Deed(done: String(localized: "Looked at"), doing: String(localized: "looking at"), symbol: "eye"),
        .read: Deed(done: String(localized: "Read"), doing: String(localized: "reading"), symbol: "text.alignleft"),
        .type: Deed(done: String(localized: "Typed in"), doing: String(localized: "typing in"), symbol: "keyboard",
                    saying: String(localized: "typing"), said: String(localized: "typed")),
        .press: Deed(done: String(localized: "Pressed keys in"), doing: String(localized: "pressing keys in"), symbol: "command",
                     saying: String(localized: "pressing"), said: String(localized: "pressed")),
        .start: Deed(done: String(localized: "Started"), doing: String(localized: "starting"), symbol: "play.fill"),
        .interrupt: Deed(done: String(localized: "Interrupted"), doing: String(localized: "interrupting"), symbol: "stop.fill"),
        .close: Deed(done: String(localized: "Closed"), doing: String(localized: "closing"), symbol: "xmark"),
        .show: Deed(done: String(localized: "Showed"), doing: String(localized: "showing"), symbol: "rectangle.on.rectangle"),
        .wait: Deed(done: String(localized: "Waited for"), doing: String(localized: "waiting for"), symbol: "hourglass"),
        .post: Deed(done: String(localized: "Posted in"), doing: String(localized: "posting in"), symbol: "bubble.left.fill",
                    saying: String(localized: "posting"), said: String(localized: "posted")),
        .run: Deed(done: String(localized: "Ran"), doing: String(localized: "running"), symbol: "chevron.right"),
        .write: Deed(done: String(localized: "Wrote"), doing: String(localized: "writing"), symbol: "square.and.pencil"),
        .edit: Deed(done: String(localized: "Changed"), doing: String(localized: "changing"), symbol: "pencil"),
        .search: Deed(done: String(localized: "Searched for"), doing: String(localized: "searching for"), symbol: "magnifyingglass"),
        .fetch: Deed(done: String(localized: "Fetched"), doing: String(localized: "fetching"), symbol: "globe"),
        .plan: Deed(done: String(localized: "Planned"), doing: String(localized: "planning"), symbol: "checklist"),
        .delegate: Deed(done: String(localized: "Asked a helper"), doing: String(localized: "asking a helper"), symbol: "person.2"),
        .other: other,
    ]

    private static let other = Deed(done: String(localized: "Used"), doing: String(localized: "using"), symbol: "wrench.and.screwdriver")

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

    static func doing(_ tool: DosiTool) -> String {
        let verb = deed(tool.action).doing
        guard let target = tool.target.map(target), !target.isEmpty else { return verb }
        return "\(verb) \(target)"
    }

    static func receipt(_ tool: DosiTool) -> String {
        let deed = deed(tool.action)
        let place = tool.target.map(target).flatMap { $0.isEmpty ? nil : $0 }
        let verb = tool.status == .running ? deed.doing : deed.done.lowercased()
        guard tool.text?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else {
            return place.map { "\(verb) \($0)" } ?? verb
        }
        let said = said(tool)
        return place.map { "\(said) · \($0)" } ?? said
    }

    static func said(_ tool: DosiTool) -> String {
        let deed = deed(tool.action)
        let verb = tool.status == .running ? deed.saying ?? deed.doing : deed.said ?? deed.done.lowercased()
        guard let text = tool.text?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else { return verb }
        let line = text.split(separator: "\n").first.map(String.init) ?? text
        return "\(verb) \(line)"
    }

    static func summary(_ tools: [DosiTool]) -> String {
        let verb = verb(tools)
        var names: [String] = []
        for name in tools.compactMap({ $0.target.map(target) }) where !name.isEmpty && !names.contains(name) {
            names.append(name)
        }
        guard !names.isEmpty else { return verb }
        let shown = names.prefix(2).joined(separator: ", ")
        return names.count > 2 ? "\(verb) \(shown) +\(names.count - 2)" : "\(verb) \(shown)"
    }

    private static func verb(_ tools: [DosiTool]) -> String {
        let running = tools.contains { $0.status == .running }
        let actions = Set(tools.map(\.action))
        guard actions.count == 1, let action = actions.first else {
            return running ? String(localized: "working on") : String(localized: "worked on")
        }
        return running ? deed(action).doing : deed(action).done.lowercased()
    }
}
