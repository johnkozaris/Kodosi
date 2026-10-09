import SwiftUI

struct MentionTarget: Identifiable, Equatable {
    enum Kind: Equatable {
        case person(isSelf: Bool)
        case terminal(AgentKind)
    }

    let id: String
    let name: String
    var detail: String?
    let kind: Kind

    var isSelf: Bool {
        kind == .person(isSelf: true)
    }

    var isTerminal: Bool {
        if case .terminal = kind {
            return true
        }
        return false
    }

    var link: URL? {
        isTerminal ? URL(string: "kodosi-mention://terminal/\(id)") : nil
    }
}

enum MentionSegment: Equatable {
    case text(String)
    case mention(MentionTarget)
}

enum RoomMentions {
    static func targets(members: [MissionMemberEntry], sessions: [RuntimeSession], userId: String?) -> [MentionTarget] {
        let people = members.map { member in
            MentionTarget(id: member.userId, name: member.displayName.isEmpty ? member.handle : member.displayName,
                          detail: "@\(member.handle)", kind: .person(isSelf: member.userId == userId))
        }
        let terminals = sessions.map { session in
            MentionTarget(id: session.id, name: session.name, detail: session.agent.isAgent ? session.agent.label : session.hostLabel,
                          kind: .terminal(session.agent))
        }
        return people + terminals
    }

    static func segments(_ text: String, targets: [MentionTarget]) -> [MentionSegment] {
        guard text.contains("@"), !targets.isEmpty else { return [.text(text)] }
        let ordered = targets.sorted { $0.name.count > $1.name.count }
        var result: [MentionSegment] = []
        var pending = ""
        var index = text.startIndex
        while index < text.endIndex {
            let character = text[index]
            let startsWord = index == text.startIndex || !isWord(text[text.index(before: index)])
            if character == "@", startsWord, let (target, end) = match(text, after: text.index(after: index), in: ordered) {
                if !pending.isEmpty {
                    result.append(.text(pending))
                    pending = ""
                }
                result.append(.mention(target))
                index = end
            } else {
                pending.append(character)
                index = text.index(after: index)
            }
        }
        if !pending.isEmpty {
            result.append(.text(pending))
        }
        return result
    }

    static func mentionsSelf(_ text: String, targets: [MentionTarget]) -> Bool {
        segments(text, targets: targets).contains { segment in
            if case let .mention(target) = segment {
                return target.isSelf
            }
            return false
        }
    }

    static func query(in draft: String) -> String? {
        guard let at = draft.lastIndex(of: "@") else { return nil }
        if at != draft.startIndex, isWord(draft[draft.index(before: at)]) {
            return nil
        }
        let tail = draft[draft.index(after: at)...]
        guard tail.count <= 40, !tail.contains("\n"), tail.split(separator: " ", omittingEmptySubsequences: false).count <= 3 else { return nil }
        return String(tail)
    }

    static func complete(_ draft: String, with target: MentionTarget) -> String {
        guard let at = draft.lastIndex(of: "@") else { return draft }
        return String(draft[..<at]) + "@\(target.name) "
    }

    static func suggestions(for query: String, in targets: [MentionTarget]) -> [MentionTarget] {
        let needle = query.lowercased()
        let candidates = targets.filter { !$0.isSelf }
        guard !needle.isEmpty else { return Array(candidates.prefix(6)) }
        let starts = candidates.filter { $0.name.lowercased().hasPrefix(needle) }
        let contains = candidates.filter { !$0.name.lowercased().hasPrefix(needle) && $0.name.lowercased().contains(needle) }
        return Array((starts + contains).prefix(6))
    }

    private static func isWord(_ character: Character) -> Bool {
        character.isLetter || character.isNumber || character == "_"
    }

    private static func match(_ text: String, after start: String.Index, in targets: [MentionTarget]) -> (MentionTarget, String.Index)? {
        let rest = text[start...]
        for target in targets {
            guard let range = rest.range(of: target.name, options: [.caseInsensitive, .anchored]) else { continue }
            if range.upperBound == text.endIndex || !isWord(text[range.upperBound]) {
                return (target, range.upperBound)
            }
        }
        return nil
    }
}

struct MentionChip: TextAttribute {
    let color: Color
}

struct MentionRenderer: TextRenderer {
    func draw(layout: Text.Layout, in context: inout GraphicsContext) {
        for line in layout {
            for run in line {
                if let chip = run[MentionChip.self] {
                    let rect = run.typographicBounds.rect.insetBy(dx: -3, dy: 0)
                    context.fill(RoundedRectangle(cornerRadius: 5, style: .continuous).path(in: rect), with: .color(chip.color.opacity(0.18)))
                }
                context.draw(run)
            }
        }
    }
}

struct RoomMessageText: View {
    @Environment(\.theme) private var theme
    let text: String
    let targets: [MentionTarget]

    var body: some View {
        let segments = RoomMentions.segments(text, targets: targets)
        if segments.count == 1, case .text = segments[0] {
            Text(.init(text))
        } else {
            segments.reduce(Text(verbatim: "")) { result, segment in
                switch segment {
                case let .text(value): Text("\(result)\(Text(.init(value)))")
                case let .mention(target): Text("\(result)\(chip(target))")
                }
            }
            .textRenderer(MentionRenderer())
        }
    }

    private func chip(_ target: MentionTarget) -> Text {
        let color = color(target)
        var label = AttributedString("@\(target.name)")
        label.foregroundColor = color
        label.font = .system(size: 13, weight: .semibold)
        if let link = target.link {
            label.link = link
        }
        return Text(label).customAttribute(MentionChip(color: color))
    }

    private func color(_ target: MentionTarget) -> Color {
        switch target.kind {
        case let .person(isSelf): isSelf ? theme.colors.accentStrong : PersonTint.color(for: target.id).mix(with: theme.colors.ink, by: 0.25)
        case let .terminal(kind): kind.isAgent ? kind.tint.mix(with: theme.colors.ink, by: 0.2) : theme.colors.accentStrong
        }
    }
}
