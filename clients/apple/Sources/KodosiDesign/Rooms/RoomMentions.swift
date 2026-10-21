import Foundation
import KodosiBridge

public struct RoomMention: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case person(isSelf: Bool)
        case terminal(AgentKind)
    }

    public let id: String
    public let name: String
    public var detail: String?
    public let kind: Kind

    public init(id: String, name: String, detail: String? = nil, kind: Kind) {
        self.id = id
        self.name = name
        self.detail = detail
        self.kind = kind
    }

    public var isSelf: Bool {
        kind == .person(isSelf: true)
    }

    public var isTerminal: Bool {
        if case .terminal = kind {
            return true
        }
        return false
    }

    public var link: URL? {
        isTerminal ? RoomMentions.terminalLink(id) : nil
    }
}

public enum RoomMentionSegment: Equatable, Sendable {
    case text(String)
    case mention(RoomMention)
}

public enum RoomMentions {
    static let scheme = "kodosi-room"

    public static func terminalLink(_ id: String) -> URL? {
        var components = URLComponents()
        components.scheme = scheme
        components.host = "terminal"
        components.path = "/\(id)"
        return components.url
    }

    public static func terminalId(in url: URL) -> String? {
        guard url.scheme == scheme, url.host() == "terminal" else { return nil }
        let id = url.lastPathComponent
        return id.isEmpty || id == "/" ? nil : id
    }

    public static func targets(members: [MissionMemberEntry], sessions: [RuntimeSession], userId: String?) -> [RoomMention] {
        let people = members.map { member in
            RoomMention(id: member.userId, name: member.personName, detail: "@\(member.handle)", kind: .person(isSelf: member.userId == userId))
        }
        let terminals = sessions.map { session in
            RoomMention(
                id: session.id, name: session.name, detail: session.agent.isAgent ? session.agent.label : session.hostLabel,
                kind: .terminal(session.agent),
            )
        }
        return people + terminals
    }

    public static func segments(_ text: String, targets: [RoomMention]) -> [RoomMentionSegment] {
        guard text.contains("@"), !targets.isEmpty else { return [.text(text)] }
        let ordered = targets.filter { !$0.name.isEmpty }.sorted { $0.name.count > $1.name.count }
        var result: [RoomMentionSegment] = []
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

    public static func mentionsSelf(_ text: String, targets: [RoomMention]) -> Bool {
        segments(text, targets: targets).contains { segment in
            if case let .mention(target) = segment {
                return target.isSelf
            }
            return false
        }
    }

    public static func query(in draft: String) -> String? {
        guard let at = draft.lastIndex(of: "@") else { return nil }
        if at != draft.startIndex, isWord(draft[draft.index(before: at)]) {
            return nil
        }
        let tail = draft[draft.index(after: at)...]
        guard tail.count <= 40, !tail.contains(where: \.isNewline),
              tail.split(separator: " ", omittingEmptySubsequences: false).count <= 3
        else { return nil }
        return String(tail)
    }

    public static func complete(_ draft: String, with target: RoomMention) -> String {
        guard let at = draft.lastIndex(of: "@") else { return draft }
        return String(draft[..<at]) + "@\(target.name) "
    }

    public static func start(_ draft: String) -> String {
        guard query(in: draft) == nil else { return draft }
        return draft + (draft.isEmpty || draft.hasSuffix(" ") || draft.hasSuffix("\n") ? "@" : " @")
    }

    public static func suggestions(for query: String, in targets: [RoomMention]) -> [RoomMention] {
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

    private static func match(_ text: String, after start: String.Index, in targets: [RoomMention]) -> (RoomMention, String.Index)? {
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
