import KodosiBridge
import SwiftUI

/// A message that waits for the agent, and when the agent reads it. Its words select and copy as in
/// the conversation, and a key takes it back while the agent does not have it yet.
struct ChatQueuedRow: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let queued: ChatQueued
    let agent: AgentKind
    let showTerminal: () -> Void

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            HStack(alignment: .center, spacing: 6) {
                Spacer(minLength: 40)
                if queued.given == nil {
                    IconButton(title: "Remove", bundle: .module, symbol: "xmark",
                               identifier: "chat.queued.remove.\(AccessibilityIdentifier.token(queued.id))", size: 22)
                    {
                        chat.unqueue(queued)
                    }
                }
                let bubble = HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: sign)
                        .font(mac: 9, ios: .caption2, weight: .semibold)
                        .foregroundStyle(queued.refused != nil && !queued.terminal ? theme.colors.caution : theme.colors.inkFaint)
                        .accessibilityLabel(status)
                    if let handoff = queued.said.handoff {
                        ChatHandOffLine(handoff: handoff, style: .footnote)
                    } else {
                        if !queued.said.text.isEmpty {
                            Text(queued.said.text).appTextStyle(.footnote).foregroundStyle(theme.colors.ink).lineLimit(2)
                                .messageTextSelection()
                        }
                        if !queued.said.pictures.isEmpty {
                            ChatPictureTiles(count: queued.said.pictures.count, size: 20)
                        }
                    }
                }
                .projectBubble()
                .help(status)
                if queued.said.handoff == nil, !queued.said.text.isEmpty {
                    bubble.copiesMessage(queued.said.text, preview: ProjectBubble())
                } else {
                    bubble
                }
            }
            if queued.terminal {
                Button(action: showTerminal) {
                    Tag(text: String(localized: "Command needs terminal view", bundle: .module), symbol: "apple.terminal", tone: .accent)
                        .touchTarget(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("chat.queued.terminal")
            }
        }
        .accessibilityElement(children: .contain)
    }

    private var sign: String {
        if queued.terminal {
            return "apple.terminal"
        }
        if queued.refused != nil {
            return "exclamationmark.triangle"
        }
        return queued.given == nil ? "clock" : "checkmark"
    }

    private var status: Text {
        if queued.terminal {
            return Text("Command needs terminal view", bundle: .module)
        }
        if let refused = queued.refused {
            return Text(verbatim: refused)
        }
        return switch queued.given {
        case .step: Text("\(agent.label) reads this before its next step.", bundle: .module)
        case .turn: Text("\(agent.label) reads this after its turn.", bundle: .module)
        case nil: Text("Waiting for \(agent.label)", bundle: .module)
        }
    }
}
