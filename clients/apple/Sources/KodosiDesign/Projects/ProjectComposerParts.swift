import KodosiBridge
import SwiftUI

public struct ProjectMentionCompletion: Equatable {
    private var dismissed: String?
    public private(set) var highlighted = 0

    public init() {}

    public func query(in draft: String, focused: Bool) -> String? {
        guard focused, let query = ProjectMentions.query(in: draft), query != dismissed else { return nil }
        return query
    }

    public func suggestions(in draft: String, focused: Bool, targets: [ProjectMention]) -> [ProjectMention] {
        query(in: draft, focused: focused).map { ProjectMentions.suggestions(for: $0, in: targets) } ?? []
    }

    public func current(in suggestions: [ProjectMention]) -> ProjectMention? {
        suggestions.isEmpty ? nil : suggestions[min(highlighted, suggestions.count - 1)]
    }

    public mutating func start(in draft: inout String) {
        dismissed = nil
        draft = ProjectMentions.start(draft)
    }

    public mutating func pick(_ target: ProjectMention, in draft: inout String) {
        draft = ProjectMentions.complete(draft, with: target)
        highlighted = 0
    }

    public mutating func submit(_ draft: inout String, suggestions: [ProjectMention]) -> Bool {
        guard let target = current(in: suggestions) else { return true }
        pick(target, in: &draft)
        return false
    }

    public mutating func sent() {
        dismissed = nil
        highlighted = 0
    }

    fileprivate mutating func move(by offset: Int, in suggestions: [ProjectMention]) -> Bool {
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
    func projectComposerKeys(
        _ completion: Binding<ProjectMentionCompletion>, state: ProjectViewState, focused: Bool, suggestions: [ProjectMention],
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

public struct ProjectMentionSuggestions: View {
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
    let suggestions: [ProjectMention]
    let highlighted: ProjectMention?
    let identifier: String
    let pick: (ProjectMention) -> Void

    public init(suggestions: [ProjectMention], highlighted: ProjectMention?, identifier: String, pick: @escaping (ProjectMention) -> Void) {
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

    private func row(_ target: ProjectMention, highlighted: Bool) -> some View {
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
