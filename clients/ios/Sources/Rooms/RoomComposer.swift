import KodosiDesign
import SwiftUI

struct RoomComposer: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    let targets: [RoomMention]
    @State private var dismissedQuery: String?
    @FocusState private var focused: Bool

    var body: some View {
        @Bindable var state = model.rooms.view(roomId)
        let query = focused ? RoomMentions.query(in: state.message).flatMap { $0 == dismissedQuery ? nil : $0 } : nil
        let suggestions = query.map { RoomMentions.suggestions(for: $0, in: targets) } ?? []
        let canSend = !state.sending && !state.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        VStack(spacing: 8) {
            if let failure = state.failure {
                failureNote(failure, state: state)
            }
            if !suggestions.isEmpty {
                suggestionList(suggestions, state: state)
            }
            HStack(alignment: .bottom, spacing: 4) {
                IconButton(title: "Mention someone or a terminal", symbol: "at", identifier: "rooms.composer.mention", size: 34) {
                    dismissedQuery = nil
                    state.message = RoomMentions.start(state.message)
                    focused = true
                }
                TextField("Message the room", text: $state.message, axis: .vertical)
                    .textFieldStyle(.plain)
                    .tint(theme.colors.accent)
                    .lineLimit(1 ... 6)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .focused($focused)
                    .padding(.vertical, 7)
                    .accessibilityLabel(Text("Message the room"))
                    .accessibilityIdentifier("rooms.composer.input")
                    .onKeyPress(.return, phases: .down) { press in
                        guard press.modifiers.isDisjoint(with: [.shift, .option]) else { return .ignored }
                        if let first = suggestions.first {
                            pick(first, state: state)
                        } else if canSend {
                            send()
                        }
                        return .handled
                    }
                    .onKeyPress(.escape) {
                        guard let query else { return .ignored }
                        dismissedQuery = query
                        return .handled
                    }
                SendKey(enabled: canSend, working: state.sending, size: 34, action: send)
                    .accessibilityLabel(Text("Send message"))
                    .accessibilityIdentifier("rooms.composer.send")
            }
            .composerField(focused: focused)
        }
        .padding(.horizontal, 12)
        .padding(.top, 6)
        .padding(.bottom, 8)
        .frame(maxWidth: 760)
        .frame(maxWidth: .infinity)
        .animation(theme.motion.snappy, value: suggestions)
        .animation(theme.motion.hover, value: focused)
        .animation(theme.motion.snappy, value: canSend)
        .animation(theme.motion.spring, value: state.failure)
        .sensoryFeedback(.impact(weight: .light), trigger: state.sent)
    }

    private func failureNote(_ failure: String, state: RoomViewState) -> some View {
        CautionNote(message: failure, lines: 3, identifier: "rooms.composer.failure") {
            state.failure = nil
        } actions: {
            Button("Try again", action: send)
                .buttonStyle(.kodosi(.tinted, size: .small))
                .accessibilityIdentifier("rooms.composer.retry")
        }
        .transition(AnyTransition.rise)
    }

    private func suggestionList(_ suggestions: [RoomMention], state: RoomViewState) -> some View {
        VStack(spacing: 1) {
            ForEach(Array(suggestions.enumerated()), id: \.element.id) { index, target in
                Button { pick(target, state: state) } label: {
                    suggestionRow(target, highlighted: index == 0)
                }
                .buttonStyle(PressScaleStyle(scale: 0.98))
                .accessibilityIdentifier("rooms.mention.\(AccessibilityIdentifier.token(target.id))")
            }
        }
        .padding(5)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .lifted)
        .transition(.scale(scale: 0.96, anchor: .bottomLeading).combined(with: .opacity))
    }

    private func suggestionRow(_ target: RoomMention, highlighted: Bool) -> some View {
        HStack(spacing: 10) {
            switch target.kind {
            case .person: PersonAvatar(name: target.name, key: target.id, size: 26)
            case let .terminal(kind): AgentMark(kind: kind, size: 26)
            }
            Text(target.name).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(1)
            if let detail = target.detail {
                Text(detail).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
            }
            Spacer(minLength: 4)
            if highlighted {
                Image(systemName: "return").font(.system(size: 10, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 44)
        .background(highlighted ? theme.colors.accentSoft : .clear, in: RoundedRectangle(cornerRadius: Radius.sm, style: .continuous))
        .contentShape(Rectangle())
    }

    private func pick(_ target: RoomMention, state: RoomViewState) {
        state.message = RoomMentions.complete(state.message, with: target)
    }

    private func send() {
        dismissedQuery = nil
        model.rooms.view(roomId).followsLatest = true
        let rooms = model.rooms
        let roomId = roomId
        Task { await rooms.post(in: roomId) }
    }
}
