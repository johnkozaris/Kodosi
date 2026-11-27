import KodosiBridge
import KodosiLinks
import SwiftUI

/// Rows keep the name of the agent that wrote them after the agent ends; until the agent reports,
/// `agent` (the terminal's program) names them. Pass the signed-in user ID as `me` so messages
/// from other participants display their names. `modelKey` builds the control that changes the
/// agent's model and effort, which `/model` also opens. `showTerminal` shows the terminal's screen
/// in place of the chat. Links open by the same rules as links in the terminal, and
/// `otherComputer` names the computer of a terminal that is not on this device, where its files
/// are. `names`, such as the terminal's and its project's, help talking spell them right.
/// `goOn` lets an agent that a restart stopped in the middle of a task go on. `openTerminal` opens
/// the terminal of an agent that a message names.
public struct TerminalChatView<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let agent: AgentKind
    let me: String?
    let otherComputer: String?
    let names: [String]
    let openTerminal: ((String) -> Void)?
    let goOn: (() -> Void)?
    let showTerminal: () -> Void
    let modelKey: (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey
    @FocusState private var focused: Bool
    @State private var sent = 0

    public init(
        chat: TerminalChat, agent: AgentKind, me: String?, otherComputer: String?, names: [String] = [],
        openTerminal: ((String) -> Void)? = nil, goOn: (() -> Void)? = nil, showTerminal: @escaping () -> Void,
        @ContentBuilder modelKey: @escaping (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey,
    ) {
        self.chat = chat
        self.agent = agent
        self.me = me
        self.otherComputer = otherComputer
        self.names = names
        self.openTerminal = openTerminal
        self.goOn = goOn
        self.showTerminal = showTerminal
        self.modelKey = modelKey
    }

    public var body: some View {
        VStack(spacing: 0) {
            ChatThread(chat: chat, agent: chat.facts.agent.flatMap(AgentKind.init(rawValue:)) ?? agent, me: me, sent: sent)
            ChatDock(chat: chat, agent: agent, names: names, focused: $focused, modelKey: modelKey, goOn: goOn,
                     showTerminal: showTerminal)
            {
                sent += 1
            }
            .layoutPriority(1)
        }
        .background(theme.colors.chat)
        .takesPictures(chat)
        .environment(\.proseFolder, chat.facts.folder)
        .environment(\.openURL, OpenURLAction { url in
            if let terminal = ProjectMentions.terminalId(in: url) {
                openTerminal?(terminal)
                return .handled
            }
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

/// The two ways to bring the agent of another terminal into a chat.
public enum ChatSymbol {
    public static let mention = "at"
    public static let handOff = "arrow.turn.down.right"
}

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
