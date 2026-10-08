import SwiftUI

struct RoomComposer: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @Bindable var state: RoomViewState
    let targets: [MentionTarget]
    @State private var highlighted = 0
    @State private var dismissedQuery: String?
    @FocusState private var focused: Bool

    private var query: String? {
        guard focused, let query = RoomMentions.query(in: state.message), query != dismissedQuery else { return nil }
        return query
    }

    private var canSend: Bool {
        !state.busy && !state.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        let suggestions = query.map { RoomMentions.suggestions(for: $0, in: targets) } ?? []
        VStack(spacing: 6) {
            if let failure = state.failure {
                HStack(spacing: 8) {
                    Circle().fill(theme.colors.caution).frame(width: 6, height: 6)
                    Text(failure).appTextStyle(.footnote).foregroundStyle(theme.colors.ink).lineLimit(2)
                    Spacer(minLength: 4)
                    IconButton(title: "Dismiss", symbol: "xmark", identifier: "room.failure.dismiss", size: 22) { state.failure = nil }
                }
                .padding(.leading, 12).padding(.trailing, 4).padding(.vertical, 6)
                .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
                .transition(AnyTransition.rise)
            }
            if !suggestions.isEmpty {
                VStack(spacing: 1) {
                    ForEach(Array(suggestions.enumerated()), id: \.element.id) { index, target in
                        Button { pick(target) } label: { suggestionRow(target, highlighted: index == min(highlighted, suggestions.count - 1)) }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("room.mention.\(AccessibilityIdentifier.token(target.id))")
                    }
                }
                .padding(5)
                .raised(Radius.lg, fill: theme.colors.lifted, elevation: .lifted)
                .transition(.scale(scale: 0.96, anchor: .bottomLeading).combined(with: .opacity))
            }
            HStack(alignment: .bottom, spacing: 4) {
                IconButton(title: "Mention someone or a terminal", symbol: "at", identifier: "room.message.mention", size: 30) {
                    dismissedQuery = nil
                    if RoomMentions.query(in: state.message) == nil {
                        state.message += state.message.isEmpty || state.message.hasSuffix(" ") ? "@" : " @"
                    }
                    focused = true
                }
                TextField("Message the room", text: $state.message, axis: .vertical)
                    .lineLimit(1 ... 6).textFieldStyle(.plain).appTextStyle(.callout).focused($focused)
                    .padding(.vertical, 7)
                    .accessibilityLabel(Text("Message the room")).accessibilityIdentifier("room.message.input")
                    .onSubmit {
                        if let target = current(in: suggestions) {
                            pick(target)
                        } else if canSend {
                            send()
                        }
                    }
                    .onKeyPress(.return, phases: .down) { press in
                        guard press.modifiers.isDisjoint(with: [.option, .shift]) else { return .ignored }
                        if let target = current(in: suggestions) {
                            pick(target)
                        } else if canSend {
                            send()
                        }
                        return .handled
                    }
                    .onKeyPress(.downArrow) { move(1, count: suggestions.count) }
                    .onKeyPress(.upArrow) { move(-1, count: suggestions.count) }
                    .onKeyPress(.tab) {
                        guard let target = current(in: suggestions) else { return .ignored }
                        pick(target)
                        return .handled
                    }
                    .onKeyPress(.escape) {
                        guard let query else { return .ignored }
                        dismissedQuery = query
                        return .handled
                    }
                Button(action: send) {
                    Image(systemName: "arrow.up").font(.system(size: 13, weight: .bold))
                        .foregroundStyle(canSend ? theme.colors.onAccent : theme.colors.inkFaint)
                        .frame(width: 30, height: 30)
                        .background {
                            if canSend {
                                RaisedBackground(shape: Circle(), fill: theme.colors.accent, elevation: .resting)
                            } else {
                                Circle().fill(theme.colors.ink.opacity(0.08))
                            }
                        }
                        .overlay {
                            if state.busy {
                                WorkingRim(shape: Circle(), lineWidth: 2, glow: 4)
                            }
                        }
                }
                .buttonStyle(PressScaleStyle(scale: 0.9)).disabled(!canSend)
                .accessibilityLabel(Text("Send message")).accessibilityIdentifier("room.message.send")
            }
            .padding(5)
            .background {
                RaisedBackground(shape: RoundedRectangle(cornerRadius: 20, style: .continuous), fill: theme.colors.lifted,
                                 elevation: focused ? .lifted : .resting)
            }
            .overlay {
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .strokeBorder(theme.colors.accent.opacity(focused ? 0.5 : 0), lineWidth: 1.5)
            }
            .shadow(color: theme.colors.accent.opacity(focused ? 0.16 : 0), radius: 10)
        }
        .padding(.horizontal, 10).padding(.bottom, 10).padding(.top, 4)
        .animation(theme.motion.snappy, value: suggestions)
        .animation(theme.motion.hover, value: focused)
        .animation(theme.motion.snappy, value: canSend)
        .animation(theme.motion.spring, value: state.failure)
        .onChange(of: query) { _, _ in highlighted = 0 }
    }

    private func suggestionRow(_ target: MentionTarget, highlighted: Bool) -> some View {
        HStack(spacing: 9) {
            switch target.kind {
            case .person: PersonAvatar(name: target.name, key: target.id, size: 22)
            case let .terminal(kind): AgentMark(kind: kind, size: 22)
            }
            Text(target.name).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(1)
            if let detail = target.detail {
                Text(detail).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
            }
            Spacer(minLength: 4)
            if highlighted {
                Image(systemName: "return").font(.system(size: 9, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .padding(.horizontal, 8).frame(height: 32)
        .background(highlighted ? theme.colors.accentSoft : .clear, in: RoundedRectangle(cornerRadius: Radius.sm, style: .continuous))
        .contentShape(Rectangle())
    }

    private func current(in suggestions: [MentionTarget]) -> MentionTarget? {
        suggestions.isEmpty ? nil : suggestions[min(highlighted, suggestions.count - 1)]
    }

    private func move(_ offset: Int, count: Int) -> KeyPress.Result {
        guard count > 0 else { return .ignored }
        highlighted = (min(highlighted, count - 1) + offset + count) % count
        return .handled
    }

    private func pick(_ target: MentionTarget) {
        state.message = RoomMentions.complete(state.message, with: target)
        highlighted = 0
    }

    private func send() {
        dismissedQuery = nil
        Task { await deps.postRoomMessage(roomId) }
    }
}
