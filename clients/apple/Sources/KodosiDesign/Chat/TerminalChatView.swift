import KodosiBridge
import SwiftUI

/// The chat of a terminal in the place of its screen. It is a conversation of Kodosi, as a
/// project's, in the look of what holds it: a terminal's tile or the app. `me` is the account of
/// this app: the messages of other people show their names. `modelKey` gets the agent's model and
/// effort, and the app makes them the key of its action that changes them.
public struct TerminalChatView<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let agent: AgentKind
    let me: String?
    let modelKey: (_ model: String, _ effort: String?) -> ModelKey
    @FocusState private var focused: Bool
    @State private var sent = 0

    public init(
        chat: TerminalChat, agent: AgentKind, me: String?,
        @ContentBuilder modelKey: @escaping (_ model: String, _ effort: String?) -> ModelKey,
    ) {
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
        .background(theme.colors.chat)
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
