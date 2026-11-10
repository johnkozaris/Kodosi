import KodosiBridge
import SwiftUI

/// The mention that the person types in a room's composer, the people and terminals that complete
/// it, and the keys that pick one of them or send the message.
public struct RoomMentionCompletion: Equatable {
    private var dismissed: String?
    public private(set) var highlighted = 0

    public init() {}

    /// The words after the `@` that the person types now, unless they closed the suggestions.
    public func query(in draft: String, focused: Bool) -> String? {
        guard focused, let query = RoomMentions.query(in: draft), query != dismissed else { return nil }
        return query
    }

    public func suggestions(in draft: String, focused: Bool, targets: [RoomMention]) -> [RoomMention] {
        query(in: draft, focused: focused).map { RoomMentions.suggestions(for: $0, in: targets) } ?? []
    }

    /// The suggestion that Return or Tab picks.
    public func current(in suggestions: [RoomMention]) -> RoomMention? {
        suggestions.isEmpty ? nil : suggestions[min(highlighted, suggestions.count - 1)]
    }

    /// Starts a mention at the end of the draft.
    public mutating func start(in draft: inout String) {
        dismissed = nil
        draft = RoomMentions.start(draft)
    }

    public mutating func pick(_ target: RoomMention, in draft: inout String) {
        draft = RoomMentions.complete(draft, with: target)
        highlighted = 0
    }

    /// Return: picks the current suggestion, or says that the message goes.
    public mutating func submit(_ draft: inout String, suggestions: [RoomMention]) -> Bool {
        guard let target = current(in: suggestions) else { return true }
        pick(target, in: &draft)
        return false
    }

    public mutating func sent() {
        dismissed = nil
        highlighted = 0
    }

    fileprivate mutating func move(by offset: Int, in suggestions: [RoomMention]) -> Bool {
        guard !suggestions.isEmpty else { return false }
        let count = suggestions.count
        highlighted = (min(highlighted, count - 1) + offset + count) % count
        return true
    }

    fileprivate mutating func dismiss(in draft: String, focused: Bool) -> Bool {
        guard let query = query(in: draft, focused: focused) else { return false }
        dismissed = query
        return true
    }

    fileprivate mutating func queryChanged() {
        highlighted = 0
    }
}

public extension View {
    /// The keys of a room's composer: Return picks the current suggestion or sends, Shift-Return
    /// and Option-Return start a new line, the arrows move through the suggestions, Tab picks one,
    /// and Escape closes them.
    func roomComposerKeys(
        _ completion: Binding<RoomMentionCompletion>, state: RoomViewState, focused: Bool, suggestions: [RoomMention],
        send: @escaping () -> Void,
    ) -> some View {
        onKeyPress(.return, phases: .down) { press in
            guard press.modifiers.isDisjoint(with: [.shift, .option]) else { return .ignored }
            if completion.wrappedValue.submit(&state.message, suggestions: suggestions), state.canPost {
                send()
            }
            return .handled
        }
        .onKeyPress(.downArrow) { completion.wrappedValue.move(by: 1, in: suggestions) ? .handled : .ignored }
        .onKeyPress(.upArrow) { completion.wrappedValue.move(by: -1, in: suggestions) ? .handled : .ignored }
        .onKeyPress(.tab) {
            guard let target = completion.wrappedValue.current(in: suggestions) else { return .ignored }
            completion.wrappedValue.pick(target, in: &state.message)
            return .handled
        }
        .onKeyPress(.escape) {
            completion.wrappedValue.dismiss(in: state.message, focused: focused) ? .handled : .ignored
        }
        .onChange(of: completion.wrappedValue.query(in: state.message, focused: focused)) {
            completion.wrappedValue.queryChanged()
        }
    }
}

/// The people and terminals that complete the mention being typed, the one that Return picks
/// highlighted.
public struct RoomMentionSuggestions: View {
    #if os(iOS)
        private static let face: CGFloat = 26
        private static let spacing: CGFloat = 10
        private static let inset: CGFloat = 10
        private static let height: CGFloat = 44
        private static let returnGlyph: CGFloat = 10
    #else
        private static let face: CGFloat = 22
        private static let spacing: CGFloat = 9
        private static let inset: CGFloat = 8
        private static let height: CGFloat = 32
        private static let returnGlyph: CGFloat = 9
    #endif

    @Environment(\.theme) private var theme
    let suggestions: [RoomMention]
    let highlighted: RoomMention?
    let identifier: String
    let pick: (RoomMention) -> Void

    public init(suggestions: [RoomMention], highlighted: RoomMention?, identifier: String, pick: @escaping (RoomMention) -> Void) {
        self.suggestions = suggestions
        self.highlighted = highlighted
        self.identifier = identifier
        self.pick = pick
    }

    public var body: some View {
        VStack(spacing: 1) {
            ForEach(suggestions) { target in
                Button { pick(target) } label: { row(target, highlighted: target.id == highlighted?.id) }
                    .buttonStyle(PressScaleStyle(scale: 0.98))
                    .accessibilityIdentifier("\(identifier).\(AccessibilityIdentifier.token(target.id))")
            }
        }
        .padding(5)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .lifted)
        .transition(.scale(scale: 0.96, anchor: .bottomLeading).combined(with: .opacity))
    }

    private func row(_ target: RoomMention, highlighted: Bool) -> some View {
        HStack(spacing: Self.spacing) {
            switch target.kind {
            case .person: PersonAvatar(name: target.name, key: target.id, size: Self.face)
            case let .terminal(kind): AgentMark(kind: kind, size: Self.face)
            }
            Text(target.name).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(1)
            if let detail = target.detail {
                Text(detail).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
            }
            Spacer(minLength: 4)
            if highlighted {
                Image(systemName: "return").font(.system(size: Self.returnGlyph, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .padding(.horizontal, Self.inset)
        .frame(minHeight: Self.height)
        .background(highlighted ? theme.colors.accentSoft : .clear, in: RoundedRectangle(cornerRadius: Radius.sm, style: .continuous))
        .contentShape(Rectangle())
    }
}
