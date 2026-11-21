import KodosiBridge
import SwiftUI

/// What sits under the conversation: the waiting messages, then the question of the agent or the
/// one live line, the composer and the facts of the session.
struct ChatDock<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    @Bindable var chat: TerminalChat
    let agent: AgentKind
    var focused: FocusState<Bool>.Binding
    let modelKey: (Text) -> ModelKey
    let sent: () -> Void
    @State private var sends = 0
    @State private var selection: TextSelection?

    private var typed: Bool {
        !chat.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var stops: Bool {
        chat.working && !typed
    }

    private var placeholder: String {
        chat.question != nil ? String(localized: "Or say what to do instead", bundle: .module)
            : String(localized: "Message \(agent.label)", bundle: .module)
    }

    var body: some View {
        let ask = chat.asks.first { $0.body != .other }
        let away = chat.chat != nil && chat.state == .away
        VStack(alignment: .leading, spacing: 8) {
            ForEach(chat.queued) { queued in
                ChatQueuedRow(chat: chat, queued: queued).transition(AnyTransition.rise)
            }
            if let ask {
                ChatAskCard(chat: chat, ask: ask).id(ask.id).transition(AnyTransition.rise)
            } else if chat.state == .working {
                ChatLiveLine(chat: chat).transition(.opacity)
            }
            if let failure = chat.failure {
                HStack(spacing: 7) {
                    Circle().fill(theme.colors.caution).frame(width: 6, height: 6).accessibilityHidden(true)
                    Text(failure).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(2)
                }
                .padding(.leading, 6)
                .transition(.opacity)
            }
            if away {
                Text("No agent runs in this terminal", bundle: .module)
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                    .frame(maxWidth: .infinity).padding(.vertical, 10)
                    .accessibilityIdentifier("chat.away")
                    .transition(.opacity)
            } else {
                composer
                ChatFactsRow(facts: chat.facts, modelKey: modelKey).padding(.horizontal, 6)
            }
        }
        .padding(.horizontal, 10).padding(.top, 4).padding(.bottom, 8)
        .animation(theme.motion.spring, value: chat.queued.map(\.id))
        .animation(theme.motion.spring, value: ask?.id)
        .animation(theme.motion.fade, value: chat.state)
        .animation(theme.motion.fade, value: chat.failure)
        .animation(theme.motion.snappy, value: stops)
        .animation(theme.motion.snappy, value: typed)
        .animation(theme.motion.hover, value: focused.wrappedValue)
        #if os(iOS)
            .sensoryFeedback(.impact(weight: .light), trigger: sends)
        #endif
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 6) {
            TextField(placeholder, text: $chat.draft, selection: $selection, axis: .vertical)
                .textFieldStyle(.plain)
                .tint(theme.colors.accent)
                .lineLimit(1 ... 8)
                .appTextStyle(.body)
                .foregroundStyle(theme.colors.ink)
                .focused(focused)
                .padding(.leading, 10).padding(.vertical, 7)
                .disabled(!chat.canWrite)
                .onKeyPress(.return, phases: .down) { press in
                    if press.modifiers.isDisjoint(with: [.shift, .option]) {
                        send()
                    } else {
                        breakLine()
                    }
                    return .handled
                }
                .onKeyPress(.escape) { .handled }
                .accessibilityIdentifier("chat.input")
            if stops {
                StopKey { chat.stop() }
                    .tip(Text("Stop", bundle: .module))
                    .accessibilityIdentifier("chat.stop")
                    .transition(AnyTransition.pop)
            } else {
                SendKey(enabled: typed && chat.canWrite, action: send)
                    .tip(Text("Send", bundle: .module))
                    .accessibilityLabel(Text("Send", bundle: .module))
                    .accessibilityIdentifier("chat.send")
                    .transition(AnyTransition.pop)
            }
        }
        .composerField(focused: focused.wrappedValue)
    }

    private func breakLine() {
        var draft = chat.draft
        let end = draft.endIndex ..< draft.endIndex
        let range = if case let .selection(range)? = selection?.indices, range.upperBound <= draft.endIndex {
            range
        } else {
            end
        }
        let offset = draft.distance(from: draft.startIndex, to: range.lowerBound)
        draft.replaceSubrange(range, with: "\n")
        chat.draft = draft
        selection = TextSelection(insertionPoint: draft.index(draft.startIndex, offsetBy: offset + 1))
    }

    private func send() {
        guard typed, chat.canWrite else { return }
        chat.submit()
        sends += 1
        sent()
    }
}

private struct ChatQueuedRow: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let queued: ChatQueued

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            Image(systemName: "clock").font(mac: 9, ios: .caption2, weight: .semibold).foregroundStyle(theme.colors.inkFaint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(queued.text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(2)
                if let refused = queued.refused {
                    Text("The agent takes the next message in its terminal.", bundle: .module)
                        .appTextStyle(.caption).foregroundStyle(theme.colors.caution)
                        .help(refused)
                }
            }
            Spacer(minLength: 2)
            IconButton(title: "Remove", bundle: .module, symbol: "xmark",
                       identifier: "chat.queued.remove.\(AccessibilityIdentifier.token(queued.id))", size: 20)
            {
                chat.unqueue(queued)
            }
        }
        .padding(.leading, 10).padding(.trailing, 3).padding(.vertical, 3).frame(minHeight: 26)
        .background { BezelBackground(shape: RoundedRectangle(cornerRadius: 13, style: .continuous), fill: theme.colors.raised, bezel: .quiet) }
        .accessibilityElement(children: .contain)
    }
}

/// While the agent works, one line says what it does now, and rolls to the next.
private struct ChatLiveLine: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat

    var body: some View {
        let words = ChatWords.now(chat.now, in: chat)
        ZStack(alignment: .leading) {
            Text(verbatim: words).font(mac: 11, ios: .caption, design: .monospaced).foregroundStyle(theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle)
                .shimmer()
                .id(words)
                .transition(AnyTransition.roll)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .controlHeight(20)
        .clipped()
        .padding(.leading, 6)
        .animation(theme.motion.snappy, value: words)
        .accessibilityIdentifier("chat.live")
    }
}

private struct ChatFactsRow<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    let facts: ChatFacts
    let modelKey: (Text) -> ModelKey

    var body: some View {
        ViewThatFits(in: .horizontal) {
            row(full: true)
            row(full: false)
        }
        .font(mac: 10.5, ios: .caption2, design: .monospaced)
        .foregroundStyle(theme.colors.inkFaint)
    }

    private func row(full: Bool) -> some View {
        HStack(spacing: 10) {
            if let model = facts.model {
                modelKey(Text(verbatim: model))
            }
            if let effort = facts.effort {
                Text(verbatim: effort)
            }
            if let mode = facts.mode.flatMap(ChatWords.mode) {
                Text(verbatim: mode)
            }
            if let context = facts.context {
                ContextMeter(context: context)
            }
            if full, let limit = UsageLimit.nearest(of: facts.limits) {
                UsageDial(limit: limit, size: 18)
            }
            if full, let cost = facts.cost, cost > 0 {
                Text(verbatim: ChatWords.cost(cost))
            }
            if facts.helpers > 0 {
                Label { Text(verbatim: "\(facts.helpers)") } icon: { Image(systemName: "person.2") }
                    .labelStyle(.titleAndIcon)
                    .tip(Text("\(facts.helpers) helpers work", bundle: .module))
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(Text("\(facts.helpers) helpers work", bundle: .module))
            }
            if facts.background > 0 {
                Label { Text(verbatim: "\(facts.background)") } icon: { Image(systemName: "arrow.triangle.2.circlepath") }
                    .labelStyle(.titleAndIcon)
                    .tip(Text("\(facts.background) commands run in the background", bundle: .module))
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(Text("\(facts.background) commands run in the background", bundle: .module))
            }
        }
        .lineLimit(1)
        .fixedSize()
    }
}

private struct ContextMeter: View {
    @Environment(\.theme) private var theme
    let context: ChatContext

    var body: some View {
        let percent = Int((context.used * 100).rounded())
        let level: UsageLevel = context.used > 0.85 ? .reached : context.used >= 0.65 ? .near : .normal
        HStack(spacing: 4) {
            UsageRing(limit: UsageLimit(hours: nil, scope: nil, used: percent, level: level, headline: false), lineWidth: 1.75)
                .frame(width: 11, height: 11)
            Text(verbatim: "\(percent)%").foregroundStyle(level == .normal ? theme.colors.inkFaint : level.color(theme.colors))
                .contentTransition(.numericText(value: Double(percent)))
        }
        .animation(theme.motion.snappy, value: percent)
        .tip(Text("Context: \(percent)% used", bundle: .module))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Context: \(percent)% used", bundle: .module))
    }
}
