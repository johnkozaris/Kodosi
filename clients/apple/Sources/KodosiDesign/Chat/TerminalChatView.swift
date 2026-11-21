import KodosiBridge
import SwiftUI

/// The chat of a terminal in the place of its screen. `me` is the account of this app: the
/// messages of other people show their names. `modelKey` gets the name of the model, and the app
/// makes it the key of its action that changes the model.
public struct TerminalChatView<ModelKey: View>: View {
    let chat: TerminalChat
    let agent: AgentKind
    let me: String?
    let modelKey: (Text) -> ModelKey
    @FocusState private var focused: Bool
    @State private var sent = 0

    public init(chat: TerminalChat, agent: AgentKind, me: String?, @ContentBuilder modelKey: @escaping (Text) -> ModelKey) {
        self.chat = chat
        self.agent = agent
        self.me = me
        self.modelKey = modelKey
    }

    public var body: some View {
        VStack(spacing: 0) {
            ChatThread(chat: chat, agent: agent, me: me, sent: sent)
            ChatDock(chat: chat, agent: agent, focused: $focused, modelKey: modelKey) { sent += 1 }
                .layoutPriority(1)
        }
        .task { await chat.watch() }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
    }
}

public enum TerminalFace: Hashable, Sendable {
    case terminal, chat
}

/// The switch between a terminal's screen and its chat.
public struct TerminalFaceSwitch: View {
    @Binding var face: TerminalFace
    let identifier: String

    public init(face: Binding<TerminalFace>, identifier: String) {
        _face = face
        self.identifier = identifier
    }

    public var body: some View {
        SegmentedPill(selection: $face, options: [
            SegmentOption(value: .terminal, title: String(localized: "Terminal", bundle: .module), symbol: "apple.terminal"),
            SegmentOption(value: .chat, title: String(localized: "Chat", bundle: .module), symbol: "bubble.left.and.text.bubble.right"),
        ], compact: true, symbolic: true, identifier: identifier)
    }
}
