import KodosiBridge
import KodosiLinks
import SwiftUI

/// The chat of a terminal in the place of its screen. It is a conversation of Kodosi, as a
/// project's, in the look of what holds it: a terminal's tile or the app. `me` is the account of
/// this app: the messages of other people show their names. `modelKey` gets the agent's model and
/// effort, and the app makes them the key of its action that changes them, which `/model` also
/// opens. `showTerminal` shows the terminal's screen in the place of the chat. A link opens by the
/// rules of a link in the terminal, and `otherComputer` names the computer of a terminal that is
/// not on this device, where its files are.
public struct TerminalChatView<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let agent: AgentKind
    let me: String?
    let otherComputer: String?
    let showTerminal: () -> Void
    let modelKey: (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey
    @FocusState private var focused: Bool
    @State private var sent = 0

    public init(
        chat: TerminalChat, agent: AgentKind, me: String?, otherComputer: String?, showTerminal: @escaping () -> Void,
        @ContentBuilder modelKey: @escaping (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey,
    ) {
        self.chat = chat
        self.agent = agent
        self.me = me
        self.otherComputer = otherComputer
        self.showTerminal = showTerminal
        self.modelKey = modelKey
    }

    public var body: some View {
        VStack(spacing: 0) {
            ChatThread(chat: chat, agent: agent, me: me, sent: sent)
            ChatDock(chat: chat, agent: agent, focused: $focused, modelKey: modelKey, showTerminal: showTerminal) { sent += 1 }
                .layoutPriority(1)
        }
        .background(theme.colors.chat)
        .environment(\.proseFolder, chat.facts.folder)
        .environment(\.openURL, OpenURLAction { url in
            let link = ClickedLink(url.absoluteString, otherComputer: otherComputer)
            if case let .open(target) = link.decision {
                return .systemAction(target)
            }
            link.reviewInKeyWindow(from: .terminal)
            return .handled
        })
        .task { await chat.watch() }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
    }
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
