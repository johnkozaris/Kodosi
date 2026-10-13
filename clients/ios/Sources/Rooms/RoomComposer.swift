import KodosiBridge
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
                IconButton(title: "Mention someone or a terminal", symbol: "at", identifier: "rooms.composer.mention", size: 36) {
                    dismissedQuery = nil
                    state.message = RoomMentions.start(state.message)
                    focused = true
                }
                TextField("Message the room", text: $state.message, axis: .vertical)
                    .lineLimit(1 ... 6)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .focused($focused)
                    .padding(.vertical, 8)
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
                sendButton(canSend: canSend, sending: state.sending)
            }
            .padding(5)
            .background {
                RaisedBackground(
                    shape: RoundedRectangle(cornerRadius: 23, style: .continuous), fill: theme.colors.lifted,
                    elevation: focused ? .lifted : .resting
                )
            }
            .overlay {
                RoundedRectangle(cornerRadius: 23, style: .continuous)
                    .strokeBorder(theme.colors.accent.opacity(focused ? 0.5 : 0), lineWidth: 1.5)
            }
            .shadow(color: theme.colors.accent.opacity(focused ? 0.16 : 0), radius: 10)
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

    private func sendButton(canSend: Bool, sending: Bool) -> some View {
        Button(action: send) {
            Image(systemName: "arrow.up")
                .font(.system(size: 16, weight: .bold))
                .foregroundStyle(canSend ? theme.colors.onAccent : theme.colors.inkFaint)
                .frame(width: 36, height: 36)
                .background {
                    if canSend {
                        BezelBackground(shape: Circle(), fill: theme.colors.accent)
                    } else {
                        Circle().fill(theme.colors.ink.opacity(0.08))
                    }
                }
                .overlay {
                    if sending {
                        WorkingRim(shape: Circle(), lineWidth: 2, glow: 4)
                    }
                }
                .contentShape(Circle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.88))
        .disabled(!canSend)
        .accessibilityLabel(Text("Send message"))
        .accessibilityIdentifier("rooms.composer.send")
    }

    private func failureNote(_ failure: String, state: RoomViewState) -> some View {
        HStack(spacing: 8) {
            Circle().fill(theme.colors.caution).frame(width: 6, height: 6).accessibilityHidden(true)
            Text(failure)
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.ink)
                .lineLimit(3)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button("Try again", action: send)
                .buttonStyle(.kodosi(.tinted, size: .small))
                .accessibilityIdentifier("rooms.composer.retry")
            IconButton(title: "Dismiss", symbol: "xmark", identifier: "rooms.composer.dismissFailure", size: 30) {
                state.failure = nil
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, 4)
        .padding(.vertical, 4)
        .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .transition(AnyTransition.rise)
    }

    private func suggestionList(_ suggestions: [RoomMention], state: RoomViewState) -> some View {
        VStack(spacing: 2) {
            ForEach(suggestions) { target in
                Button { pick(target, state: state) } label: {
                    HStack(spacing: 10) {
                        switch target.kind {
                        case .person: PersonAvatar(name: target.name, key: target.id, size: 28)
                        case let .terminal(kind): AgentMark(kind: kind, size: 28)
                        }
                        Text(target.name)
                            .appTextStyle(.body)
                            .fontWeight(.medium)
                            .foregroundStyle(theme.colors.ink)
                            .lineLimit(1)
                        if let detail = target.detail {
                            Text(detail)
                                .appTextStyle(.footnote)
                                .foregroundStyle(theme.colors.inkFaint)
                                .lineLimit(1)
                        }
                        Spacer(minLength: 4)
                    }
                    .padding(.horizontal, 10)
                    .frame(minHeight: 44)
                    .contentShape(Rectangle())
                }
                .buttonStyle(PressScaleStyle(scale: 0.98))
                .accessibilityIdentifier("rooms.mention.\(AccessibilityIdentifier.token(target.id))")
            }
        }
        .padding(5)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .lifted)
        .transition(.scale(scale: 0.96, anchor: .bottomLeading).combined(with: .opacity))
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
