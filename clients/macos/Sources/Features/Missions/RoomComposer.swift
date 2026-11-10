import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomComposer: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @Bindable var state: RoomViewState
    let targets: [RoomMention]
    @State private var completion = RoomMentionCompletion()
    @FocusState private var focused: Bool

    var body: some View {
        let suggestions = completion.suggestions(in: state.message, focused: focused, targets: targets)
        VStack(spacing: 6) {
            if let failure = state.failure ?? state.readFailure {
                CautionNote(message: failure, identifier: "room.failure") {
                    state.failure = nil
                    state.readFailure = nil
                }
                .transition(AnyTransition.rise)
            }
            if !suggestions.isEmpty {
                RoomMentionSuggestions(suggestions: suggestions, highlighted: completion.current(in: suggestions), identifier: "room.mention") {
                    completion.pick($0, in: &state.message)
                }
            }
            HStack(alignment: .bottom, spacing: 4) {
                IconButton(title: "Mention someone or a terminal", symbol: "at", identifier: "room.message.mention", size: 30) {
                    completion.start(in: &state.message)
                    focused = true
                }
                TextField("Message the project", text: $state.message, axis: .vertical)
                    .lineLimit(1 ... 6).textFieldStyle(.plain).appTextStyle(.callout).focused($focused)
                    .padding(.vertical, 7)
                    .accessibilityLabel(Text("Message the project")).accessibilityIdentifier("room.message.input")
                    .onSubmit {
                        if completion.submit(&state.message, suggestions: suggestions), state.canPost {
                            send()
                        }
                    }
                    .roomComposerKeys($completion, state: state, focused: focused, suggestions: suggestions, send: send)
                SendKey(enabled: state.canPost, working: state.sending, action: send)
                    .accessibilityLabel(Text("Send message")).accessibilityIdentifier("room.message.send")
            }
            .composerField(focused: focused)
        }
        .padding(.horizontal, 10).padding(.bottom, 10).padding(.top, 4)
        .animation(theme.motion.snappy, value: suggestions)
        .animation(theme.motion.hover, value: focused)
        .animation(theme.motion.snappy, value: state.canPost)
        .animation(theme.motion.spring, value: state.failure ?? state.readFailure)
    }

    private func send() {
        completion.sent()
        let rooms = deps.rooms
        let roomId = roomId
        Task { await rooms.post(in: roomId) }
    }
}
