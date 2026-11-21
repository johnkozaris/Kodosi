import Foundation
import KodosiBridge

enum ChatWords {
    private static var locale: Locale {
        Locale(identifier: AppLanguage.current)
    }

    static func deed(_ kind: ChatToolKind) -> DosiWords.Deed {
        DosiWords.deed(action(kind))
    }

    private static func action(_ kind: ChatToolKind) -> DosiTool.Action {
        switch kind {
        case .command: .run
        case .read: .read
        case .edit: .edit
        case .write: .write
        case .search: .search
        case .web: .fetch
        case .agent: .delegate
        case .plan: .plan
        case .skill, .question, .other: .other
        }
    }

    static func symbol(_ kind: ChatToolKind) -> String {
        switch kind {
        case .question: "questionmark.bubble"
        case .skill: "sparkles"
        default: deed(kind).symbol
        }
    }

    /// What the call does or did, with its subject.
    static func step(_ tool: ChatTool) -> String {
        let subject = subject(of: tool)
        if tool.kind == .question, case let .question(questions, answers)? = tool.detail, let first = questions.first {
            return answers.first.map { "\(first.question) · \($0)" } ?? first.question
        }
        let deed = deed(tool.kind)
        return tool.state.isOpen ? deed.doing(subject) : deed.done(subject)
    }

    private static func subject(of tool: ChatTool) -> String? {
        let target = tool.target.split(separator: "\n").first.map(String.init) ?? ""
        if !target.isEmpty {
            return target
        }
        return [.skill, .other].contains(tool.kind) ? tool.name : nil
    }

    /// One sentence for the calls of a turn, such as "Ran 3 commands and changed 2 files".
    static func summary(_ tools: [ChatTool]) -> String {
        var order: [ChatToolKind] = []
        var counts: [ChatToolKind: Int] = [:]
        var files: [ChatToolKind: Set<String>] = [:]
        for tool in tools {
            if counts[tool.kind] == nil {
                order.append(tool.kind)
            }
            if [.read, .edit, .write].contains(tool.kind), !tool.target.isEmpty {
                files[tool.kind, default: []].insert(tool.target)
                counts[tool.kind] = files[tool.kind]?.count
            } else {
                counts[tool.kind, default: 0] += 1
            }
        }
        let clauses = order.map { clause($0, counts[$0] ?? 1) }
        return sentence(clauses.formatted(.list(type: .and).locale(locale)))
    }

    private static func clause(_ kind: ChatToolKind, _ count: Int) -> String {
        (clauses[kind] ?? clauses[.other])?(count) ?? ""
    }

    private static let clauses: [ChatToolKind: @Sendable (Int) -> String] = [
        .command: { String(localized: "ran \($0) commands", bundle: .module) },
        .read: { String(localized: "read \($0) files", bundle: .module) },
        .edit: { String(localized: "changed \($0) files", bundle: .module) },
        .write: { String(localized: "wrote \($0) files", bundle: .module) },
        .search: { String(localized: "made \($0) searches", bundle: .module) },
        .web: { String(localized: "fetched \($0) pages", bundle: .module) },
        .agent: { String(localized: "asked \($0) helpers", bundle: .module) },
        .skill: { String(localized: "used \($0) skills", bundle: .module) },
        .question: { String(localized: "asked \($0) questions", bundle: .module) },
        .plan: { String(localized: "made \($0) plans", bundle: .module) },
        .other: { String(localized: "used \($0) tools", bundle: .module) },
    ]

    /// The steps that did not go as the agent wanted, such as "1 failed".
    static func setbacks(_ tools: [ChatTool]) -> [(state: ChatToolState, words: String)] {
        [ChatToolState.failed, .denied, .stopped].compactMap { state in
            let count = tools.count { $0.state == state }
            guard count > 0 else { return nil }
            return switch state {
            case .failed: (state, String(localized: "\(count) failed", bundle: .module))
            case .denied: (state, String(localized: "\(count) denied", bundle: .module))
            default: (state, String(localized: "\(count) stopped", bundle: .module))
            }
        }
    }

    @MainActor
    static func now(_ now: ChatNow?, in chat: TerminalChat) -> String {
        switch now {
        case .writing: String(localized: "writing", bundle: .module)
        case .compacting: String(localized: "compacting", bundle: .module)
        case let .tool(id):
            if let item = chat.item(id), case let .tool(tool) = item.body {
                step(tool)
            } else {
                String(localized: "thinking", bundle: .module)
            }
        case .thinking, .other, nil: String(localized: "thinking", bundle: .module)
        }
    }

    static func mode(_ mode: ChatMode) -> String? {
        switch mode {
        case .ask: AgentPermissions.ask.label
        case .auto: AgentPermissions.auto.label
        case .unrestricted: AgentPermissions.unrestricted.label
        case .edits: String(localized: "Edits", bundle: .module)
        case .plan: String(localized: "Plan", bundle: .module)
        case .rules: String(localized: "Rules", bundle: .module)
        case .other: nil
        }
    }

    static func duration(_ ms: UInt64) -> String {
        Duration.milliseconds(Int64(ms)).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .narrow).locale(locale))
    }

    static func cost(_ dollars: Double) -> String {
        dollars.formatted(.currency(code: "USD").precision(.fractionLength(2)).locale(locale))
    }

    private static func sentence(_ text: String) -> String {
        guard let space = text.firstIndex(of: " ") else { return text.capitalized(with: locale) }
        return text[..<space].capitalized(with: locale) + text[space...]
    }
}
