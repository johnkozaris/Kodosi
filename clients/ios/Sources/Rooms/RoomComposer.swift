import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomComposer: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    let targets: [RoomMention]
    @State private var completion = RoomMentionCompletion()
    @FocusState private var focused: Bool

    var body: some View {
        @Bindable var state = model.rooms.view(roomId)
        let suggestions = completion.suggestions(in: state.message, focused: focused, targets: targets)
        VStack(spacing: 8) {
            if let failure = state.failure {
                failureNote(failure, state: state)
            }
            if !suggestions.isEmpty {
                RoomMentionSuggestions(suggestions: suggestions, highlighted: completion.current(in: suggestions), identifier: "rooms.mention") {
                    completion.pick($0, in: &state.message)
                }
            }
            HStack(alignment: .bottom, spacing: 4) {
                IconButton(title: "Mention someone or a terminal", symbol: "at", identifier: "rooms.composer.mention", size: 34) {
                    completion.start(in: &state.message)
                    focused = true
                }
                TextField("Message the project", text: $state.message, axis: .vertical)
                    .textFieldStyle(.plain)
                    .tint(theme.colors.accent)
                    .lineLimit(1 ... 6)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .focused($focused)
                    .padding(.vertical, 7)
                    .accessibilityLabel(Text("Message the project"))
                    .accessibilityIdentifier("rooms.composer.input")
                    .roomComposerKeys($completion, state: state, focused: focused, suggestions: suggestions, send: send)
                SendKey(enabled: state.canPost, working: state.sending, size: 34, action: send)
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
        .animation(theme.motion.snappy, value: state.canPost)
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

    private func send() {
        completion.sent()
        let rooms = model.rooms
        let roomId = roomId
        Task { await rooms.post(in: roomId) }
    }
}
