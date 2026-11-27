import KodosiBridge
import SwiftUI

/// The size of the keys after the composer's text.
private let composerKeySize: CGFloat = 30

/// What sits below the conversation: the agent's task list, the subagents working for it, queued
/// messages, the agent's question, the composer, and the session facts. A person can also talk in
/// place of the composer; `names` are the words that talking should spell right.
struct ChatDock<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    @Bindable var chat: TerminalChat
    let agent: AgentKind
    let names: [String]
    var focused: FocusState<Bool>.Binding
    let modelKey: (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey
    let goOn: (() -> Void)?
    let showTerminal: () -> Void
    let sent: () -> Void
    @State private var sends = 0
    @State private var selection: TextSelection?
    @State private var picked = 0
    @State private var modelOpen = false
    @State private var usageOpen = false
    @State private var settingsOpen = false
    /// The field's height while it is empty, so with one line: the keys line up with the middle of the
    /// last line.
    @State private var oneLine: CGFloat = 0
    @State private var talk = ChatTalk()
    @State private var canTalk = ChatTalk.available ?? false
    @State private var mentioning = ProjectMentionCompletion()

    private var stops: Bool {
        chat.working && !chat.typed
    }

    private var suggestions: [ChatCommand] {
        ChatCommandMatch.commands(chat.commands, for: chat.draft)
    }

    /// The agents of other terminals that fit the name after an `@` in the draft, unless a command
    /// fits the draft.
    private var mentions: [ProjectMention] {
        guard suggestions.isEmpty else { return [] }
        return mentioning.suggestions(in: chat.draft, focused: focused.wrappedValue, targets: chat.peers.map(ProjectMention.init(peer:)))
    }

    private var placeholder: String {
        chat.question != nil ? String(localized: "Or say what to do instead", bundle: .module)
            : String(localized: "Message \(agent.label)", bundle: .module)
    }

    var body: some View {
        let ask = chat.asks.first
        let away = chat.chat != nil && chat.state == .away
        let tasks = chat.tasks.contains { $0.state != .done } ? chat.tasks : []
        let helpers = chat.items.workingHelpers
        VStack(alignment: .leading, spacing: 8) {
            if !tasks.isEmpty {
                ChatTasks(tasks: tasks).transition(AnyTransition.rise)
            }
            if !helpers.isEmpty {
                ChatHelpers(helpers: helpers, agent: agent).transition(AnyTransition.rise)
            }
            ForEach(chat.queued) { queued in
                ChatQueuedRow(chat: chat, queued: queued, agent: agent, showTerminal: showTerminal).transition(AnyTransition.rise)
            }
            if let ask {
                // Give the prompt card layout priority so a growing composer does not squeeze it.
                ChatAskCard(chat: chat, ask: ask, agent: agent).id(ask.id).transition(AnyTransition.rise)
                    .layoutPriority(1)
            } else if let goOn, !chat.working {
                Button(action: goOn) { Text("Continue", bundle: .module) }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier("chat.continue")
                    .transition(AnyTransition.rise)
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
                let suggestions = suggestions
                let mentions = mentions
                if !suggestions.isEmpty {
                    ChatCommandList(commands: suggestions, picked: min(picked, suggestions.count - 1), choose: choose)
                        .transition(AnyTransition.rise)
                } else if !mentions.isEmpty {
                    ProjectMentionSuggestions(
                        suggestions: mentions, highlighted: mentioning.current(in: mentions), identifier: "chat.mention",
                        pick: mention, handOff: handOff,
                    )
                    .transition(AnyTransition.rise)
                }
                ChatTalkArea(talk: talk, chat: chat, stop: stopTalking, edit: edit, send: send) { composer }
                ChatFactsRow(facts: chat.facts, modelKey: modelKey, modelOpen: $modelOpen, usageOpen: $usageOpen,
                             settings: chat.commands.contains { $0.opens == .settings } ? chat : nil,
                             settingsOpen: $settingsOpen)
                    .padding(.horizontal, 6)
            }
        }
        .padding(.horizontal, 10).padding(.top, 4).padding(.bottom, 8)
        .chatColumn()
        .animation(theme.motion.spring, value: tasks.isEmpty)
        .animation(theme.motion.spring, value: helpers.map(\.id))
        .animation(theme.motion.spring, value: chat.queued.map(\.id))
        .animation(theme.motion.spring, value: ask?.id)
        .animation(theme.motion.fade, value: chat.state)
        .animation(theme.motion.fade, value: chat.failure)
        .animation(theme.motion.snappy, value: stops)
        .animation(theme.motion.snappy, value: chat.typed)
        .animation(theme.motion.hover, value: focused.wrappedValue)
        .animation(theme.motion.snappy, value: suggestions.isEmpty)
        .animation(theme.motion.snappy, value: mentions.map(\.id))
        .onChange(of: chat.draft) { _, _ in picked = 0 }
        .onChange(of: mentioning.query(in: chat.draft, focused: focused.wrappedValue)) { mentioning.queryChanged() }
        .onChange(of: chat.drafted) { focused.wrappedValue = true }
        .talking(talk, in: chat) {
            sends += 1
            sent()
        }
        .task { canTalk = await ChatTalk.findAvailable() }
        #if os(iOS)
            .sensoryFeedback(.impact(weight: .light), trigger: sends)
            // Dismiss the keyboard when a new prompt arrives so it stays visible.
            .onChange(of: ask?.id) { _, id in
                if id != nil, !chat.typed {
                    focused.wrappedValue = false
                }
            }
        #endif
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 6) {
            TextField(placeholder, text: $chat.draft, selection: $selection, axis: .vertical)
                .textFieldStyle(.plain)
                .tint(theme.colors.accent)
                .lineLimit(1 ... 8)
            #if os(macOS)
                .appTextStyle(.callout)
            #else
                .appTextStyle(.body)
            #endif
                .foregroundStyle(theme.colors.ink)
                .focused(focused)
                .padding(.leading, 10).padding(.vertical, 7)
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height in
                    if chat.draft.isEmpty {
                        oneLine = height
                    }
                }
                .disabled(!chat.canWrite)
                .onKeyPress(.return, phases: .down) { press in
                    if press.modifiers.isDisjoint(with: [.shift, .option]) {
                        if !complete() {
                            send()
                        }
                    } else {
                        breakLine()
                    }
                    return .handled
                }
                .onKeyPress(.tab) { complete() ? .handled : .ignored }
                .onKeyPress(.upArrow) { move(-1) }
                .onKeyPress(.downArrow) { move(1) }
                .onKeyPress(.escape) {
                    _ = mentioning.dismiss(in: chat.draft, focused: focused.wrappedValue)
                    return .handled
                }
                .accessibilityIdentifier("chat.input")
            HStack(spacing: 6) {
                if canTalk {
                    TalkKey(enabled: chat.canWrite, action: startTalking)
                        .transition(AnyTransition.pop)
                }
                if stops {
                    StopKey { chat.stop() }
                        .tip(Text("Stop", bundle: .module))
                        .accessibilityIdentifier("chat.stop")
                        .transition(AnyTransition.pop)
                } else {
                    SendKey(enabled: chat.typed && chat.canWrite, action: send)
                        .tip(Text("Send", bundle: .module))
                        .accessibilityLabel(Text("Send", bundle: .module))
                        .accessibilityIdentifier("chat.send")
                        .transition(AnyTransition.pop)
                }
            }
            .padding(.bottom, max(0, (oneLine - composerKeySize) / 2))
        }
        .composerField(focused: focused.wrappedValue)
    }

    /// Talking continues the words that the person typed, which go out with the first message.
    private func startTalking() {
        focused.wrappedValue = false
        talk.begin(in: chat, names: [agent.label] + names)
    }

    /// Ends talking; the words that did not go out wait in the composer. On the Mac, typing goes on
    /// from them at once, as there is no keyboard to open.
    private func stopTalking() {
        talk.end()
        #if os(macOS)
            Task { focused.wrappedValue = true }
        #endif
    }

    /// Ends talking and opens the words that did not go out for typing.
    private func edit() {
        talk.end()
        Task { focused.wrappedValue = true }
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
        guard chat.typed, chat.canWrite else { return }
        if let control = chat.control(of: chat.draft) {
            chat.draft = ""
            open(control)
            return
        }
        chat.submit()
        mentioning.sent()
        sends += 1
        sent()
    }

    /// Names the agent in the draft in place of the words after its `@`.
    private func mention(_ target: ProjectMention) {
        mentioning.pick(target, in: &chat.draft)
        selection = TextSelection(insertionPoint: chat.draft.endIndex)
        focused.wrappedValue = true
    }

    /// Hands the agent's work over in place of naming it: the words after its `@` go.
    private func handOff(_ target: ProjectMention) {
        guard let peer = chat.peers.first(where: { $0.terminal == target.id }) else { return }
        if let at = chat.draft.lastIndex(of: "@") {
            chat.draft = String(chat.draft[..<at])
        }
        mentioning.sent()
        chat.handOff(peer)
        sends += 1
        sent()
    }

    private func open(_ control: ChatControl) {
        switch control {
        case .model: modelOpen = true
        case .usage: usageOpen = true
        case .settings: settingsOpen = true
        }
    }

    private func move(_ step: Int) -> KeyPress.Result {
        let count = suggestions.count
        guard count > 0 else {
            return mentioning.move(by: step, in: mentions) ? .handled : .ignored
        }
        picked = (min(picked, count - 1) + step + count) % count
        return .handled
    }

    /// Return and Tab take the picked command while the draft is a part of its name, or the
    /// highlighted agent after an `@`.
    private func complete() -> Bool {
        let suggestions = suggestions
        guard !suggestions.isEmpty else {
            guard let target = mentioning.current(in: mentions) else { return false }
            mention(target)
            return true
        }
        let command = suggestions[min(picked, suggestions.count - 1)]
        guard chat.draft != "/\(command.name)" else { return false }
        choose(command)
        return true
    }

    /// A command that a control of the chat does opens that control, and another goes into the
    /// draft, where a person adds what it takes.
    private func choose(_ command: ChatCommand) {
        picked = 0
        if let control = command.opens {
            chat.draft = ""
            open(control)
        } else {
            let draft = "/\(command.name) "
            chat.draft = draft
            selection = TextSelection(insertionPoint: draft.endIndex)
            focused.wrappedValue = true
        }
    }
}

private struct ChatQueuedRow: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let queued: ChatQueued
    let agent: AgentKind
    let showTerminal: () -> Void

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            HStack(alignment: .center, spacing: 6) {
                Spacer(minLength: 40)
                if !queued.given {
                    IconButton(title: "Remove", bundle: .module, symbol: "xmark",
                               identifier: "chat.queued.remove.\(AccessibilityIdentifier.token(queued.id))", size: 22)
                    {
                        chat.unqueue(queued)
                    }
                }
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: sign)
                        .font(mac: 9, ios: .caption2, weight: .semibold)
                        .foregroundStyle(queued.refused != nil && !queued.terminal ? theme.colors.caution : theme.colors.inkFaint)
                        .accessibilityLabel(status)
                    if let handoff = queued.said.handoff {
                        ChatHandOffLine(handoff: handoff, style: .footnote)
                    } else {
                        Text(queued.said.text).appTextStyle(.footnote).foregroundStyle(theme.colors.ink).lineLimit(2)
                    }
                }
                .projectBubble()
                .help(status)
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
        return queued.given ? "checkmark" : "clock"
    }

    private var status: Text {
        if queued.terminal {
            return Text("Command needs terminal view", bundle: .module)
        }
        if let refused = queued.refused {
            return Text(verbatim: refused)
        }
        return queued.given
            ? Text("\(agent.label) reads this before its next step.", bundle: .module)
            : Text("Waiting for \(agent.label)", bundle: .module)
    }
}

/// A helper that the agent started for a part of its work and that works now.
struct ChatHelper: Identifiable, Equatable {
    let id: String
    /// The helper's kind, as the agent names it.
    let kind: String
    let task: String
    let steps: Int
    let doing: ChatStep?
    let said: String
}

extension [ChatItem] {
    /// The helpers that work now, in the order that the agent started them. A helper that goes on
    /// after its turn is here until it ends.
    var workingHelpers: [ChatHelper] {
        compactMap { item in
            guard case let .tool(tool) = item.body, tool.kind == .agent, tool.state == .running else { return nil }
            guard case let .agent(kind, steps, doing, said)? = tool.detail else {
                return ChatHelper(id: item.id, kind: "", task: tool.target, steps: 0, doing: nil, said: "")
            }
            return ChatHelper(id: item.id, kind: kind, task: tool.target, steps: steps, doing: doing, said: said)
        }
    }
}

/// The helpers that work for the agent: each one's task, the step that it takes and how many it
/// took. A helper's row shows what it said last at a press.
private struct ChatHelpers: View {
    @Environment(\.theme) private var theme
    let helpers: [ChatHelper]
    let agent: AgentKind
    @State private var open = true
    @State private var saying: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button { open.toggle() } label: {
                HStack(spacing: 7) {
                    Image(systemName: "person.2").font(mac: 10, ios: .caption, weight: .semibold).foregroundStyle(theme.colors.inkMuted)
                        .accessibilityHidden(true)
                    Text("\(helpers.count) helpers work", bundle: .module).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Image(systemName: "chevron.up").font(mac: 8, ios: .caption2, weight: .bold)
                        .rotationEffect(.degrees(open ? 180 : 0)).foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
                .controlHeight(22).touchTarget(Rectangle())
            }
            .buttonStyle(.plain)
            .help(open ? Text("Hide the helpers", bundle: .module) : Text("Show the helpers", bundle: .module))
            .accessibilityHint(open ? Text("Hide the helpers", bundle: .module) : Text("Show the helpers", bundle: .module))
            .accessibilityIdentifier("chat.helpers")
            if open {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(helpers) { helper in
                        ChatHelperLine(helper: helper, agent: agent, says: saying.contains(helper.id)) {
                            if saying.remove(helper.id) == nil {
                                saying.insert(helper.id)
                            }
                        }
                    }
                }
                .padding(.leading, 2).padding(.bottom, 6)
                .transition(AnyTransition.unfold)
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 2)
        .background { BezelBackground(shape: RoundedRectangle(cornerRadius: 13, style: .continuous), fill: theme.colors.raised, bezel: .quiet) }
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.snappy, value: saying)
    }
}

private struct ChatHelperLine: View {
    @Environment(\.theme) private var theme
    let helper: ChatHelper
    let agent: AgentKind
    let says: Bool
    let press: () -> Void

    var body: some View {
        Button(action: press) {
            HStack(alignment: .top, spacing: 8) {
                AgentMark(kind: agent, size: 18, activity: .working).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    title
                    if helper.doing != nil || helper.steps > 0 {
                        HStack(spacing: 6) {
                            if let doing = helper.doing {
                                StepSymbol(kind: doing.kind)
                                Text(verbatim: doing.target).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted)
                                    .lineLimit(1).truncationMode(.middle)
                            }
                            Text("\(helper.steps) steps", bundle: .module).appTextStyle(.caption).fontWeight(.regular)
                                .foregroundStyle(theme.colors.inkFaint).monospacedDigit().fixedSize()
                        }
                    }
                    if says, !helper.said.isEmpty {
                        Text(ProjectMessageText.formatted(helper.said)).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .lineLimit(6).padding(.top, 2)
                            .transition(.opacity)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(says ? Text("Hide what the helper said", bundle: .module) : Text("Show what the helper said", bundle: .module))
        .accessibilityElement(children: .combine)
        .accessibilityHint(says ? Text("Hide what the helper said", bundle: .module) : Text("Show what the helper said", bundle: .module))
    }

    private var title: some View {
        HStack(spacing: 0) {
            if !helper.kind.isEmpty {
                Text(verbatim: helper.kind).fontWeight(.semibold).foregroundStyle(theme.colors.ink).fixedSize()
            }
            if !helper.task.isEmpty {
                Text(verbatim: helper.kind.isEmpty ? helper.task : " · \(helper.task)")
                    .foregroundStyle(helper.kind.isEmpty ? theme.colors.ink : theme.colors.inkMuted)
                    .truncationMode(.tail)
            }
        }
        .appTextStyle(.footnote)
        .lineLimit(1)
    }
}

private struct ChatFactsRow<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    let facts: ChatFacts
    let modelKey: (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey
    @Binding var modelOpen: Bool
    @Binding var usageOpen: Bool
    /// The chat whose agent has its own settings, which the row's key opens.
    let settings: TerminalChat?
    @Binding var settingsOpen: Bool

    var body: some View {
        ViewThatFits(in: .horizontal) {
            line(full: true)
            VStack(alignment: .leading, spacing: 6) {
                model(full: true)
                HStack(spacing: 8) {
                    usage(full: true)
                    settingsKey
                    others
                }
            }
            .lineLimit(1)
            .fixedSize()
            line(full: false)
            ScrollView(.horizontal) { line(full: false) }.scrollIndicators(.hidden)
        }
        .monospacedDigit()
    }

    private func line(full: Bool) -> some View {
        HStack(spacing: 8) {
            model(full: full)
            usage(full: full)
            settingsKey
            others
        }
        .lineLimit(1)
        .fixedSize()
    }

    @ContentBuilder private func model(full: Bool) -> some View {
        if let model = facts.model {
            modelKey(model, facts.effort, $modelOpen)
                .environment(\.chatMode, facts.mode)
                .environment(\.chatFactsShort, !full)
                .environment(\.chatModelOpen, modelOpen)
        } else if let mode = facts.mode {
            ChatModeLabel(mode: mode, named: full)
                .padding(.horizontal, 4)
                .foregroundStyle(theme.colors.inkMuted)
        }
    }

    @ContentBuilder private var settingsKey: some View {
        if let settings {
            ChatSettingsKey(chat: settings, open: $settingsOpen)
        }
    }

    @ContentBuilder private func usage(full: Bool) -> some View {
        if facts.context != nil || !facts.limits.isEmpty || (facts.cost ?? 0) > 0 {
            ChatUsageKey(facts: facts, open: $usageOpen, named: full)
        }
    }

    private var others: some View {
        Group {
            if facts.background > 0 {
                Label { Text(verbatim: "\(facts.background)") } icon: { Image(systemName: "arrow.triangle.2.circlepath") }
                    .labelStyle(.titleAndIcon)
                    .tip(Text("\(facts.background) commands run in the background", bundle: .module))
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(Text("\(facts.background) commands run in the background", bundle: .module))
            }
        }
        .appTextStyle(.callout)
        .foregroundStyle(theme.colors.inkMuted)
    }
}

/// The permission mode of the agent: its symbol and, where there is room, its word.
struct ChatModeLabel: View {
    let mode: ChatMode
    var named = true

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: Self.symbol(mode)).font(mac: 12, ios: .subheadline, weight: .semibold)
            if named {
                Text(verbatim: ChatWords.mode(mode))
            }
        }
        .appTextStyle(.callout)
        .help(Text(verbatim: ChatWords.mode(mode)))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: ChatWords.mode(mode)))
    }

    static func symbol(_ mode: ChatMode) -> String {
        switch mode {
        case .ask: AgentPermissions.ask.symbol
        case .auto: AgentPermissions.auto.symbol
        case .unrestricted: AgentPermissions.unrestricted.symbol
        case .edits: "pencil"
        case .plan: ChatWords.symbol(.plan)
        case .rules: "checklist"
        }
    }
}

extension EnvironmentValues {
    /// The permission mode that the model key of a chat shows with the model.
    @Entry var chatMode: ChatMode?
    /// The facts row has room only for the short form of its keys.
    @Entry var chatFactsShort = false
    /// The menu of the model key is open.
    @Entry var chatModelOpen = false
}

/// The context and the nearest limit of the agent's plan, which open all that the agent reports
/// of its use.
private struct ChatUsageKey: View {
    @Environment(\.theme) private var theme
    let facts: ChatFacts
    @Binding var open: Bool
    var named = true

    var body: some View {
        Button { open.toggle() } label: {
            HStack(spacing: 6) {
                if let context = facts.context {
                    ContextMeter(context: context, named: named)
                }
                if let limit = UsageLimit.nearest(of: facts.limits) {
                    HStack(spacing: 4) {
                        UsageRing(used: limit.used, level: limit.level, lineWidth: 2).frame(width: 14, height: 14)
                        Group {
                            if named {
                                Text("\(limit.window) \(limit.used)%", bundle: .module)
                            } else {
                                Text(verbatim: "\(limit.used)%")
                            }
                        }
                        .foregroundStyle(limit.level == .normal ? theme.colors.ink : limit.level.color(theme.colors))
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(Text(verbatim: limit.spoken))
                }
                if facts.context == nil, facts.limits.isEmpty, let cost = facts.cost {
                    Text(verbatim: ChatWords.cost(cost))
                }
                Image(systemName: "chevron.down")
                    .font(scaled: 8, weight: .bold)
                    .foregroundStyle(open ? theme.colors.accentStrong : theme.colors.inkFaint)
                    .rotationEffect(.degrees(open ? 180 : 0))
                    .accessibilityHidden(true)
            }
            .appTextStyle(.callout)
            .fontWeight(.medium)
            .padding(.horizontal, -3)
        }
        .buttonStyle(.kodosi(open ? .tinted : .secondary, size: .small))
        .animation(theme.motion.snappy, value: open)
        #if os(iOS)
            .sheet(isPresented: $open) {
                NavigationStack {
                    ChatUsage(facts: facts)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                        .padding(20)
                        .navigationTitle(Text("Usage", bundle: .module))
                        .navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button { open = false } label: { Text("Done", bundle: .module) }
                            }
                        }
                }
                .presentationDetents([.medium])
            }
        #else
            .kodosiPopover(isPresented: $open, arrowEdge: .top, alignment: .leading) {
                ChatUsage(facts: facts)
            }
        #endif
            .help(Text("Usage", bundle: .module))
            .accessibilityLabel(Text("Usage", bundle: .module))
            .accessibilityIdentifier("chat.usage.key")
    }
}

struct ContextMeter: View {
    @Environment(\.theme) private var theme
    let context: ChatContext
    var named = false

    static func level(_ context: ChatContext) -> UsageLevel {
        context.used > 0.85 ? .reached : context.used >= 0.65 ? .near : .normal
    }

    var body: some View {
        let percent = Int((context.used * 100).rounded())
        let level = Self.level(context)
        HStack(spacing: 4) {
            UsageRing(used: percent, level: level, lineWidth: 2)
                .frame(width: 14, height: 14)
            Group {
                if named {
                    Text("\(percent)% context", bundle: .module)
                } else {
                    Text(verbatim: "\(percent)%")
                }
            }
            .foregroundStyle(level == .normal ? theme.colors.ink : level.color(theme.colors))
        }
        .animation(theme.motion.snappy, value: percent)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Context: \(percent)% used", bundle: .module))
    }
}

public struct ChatModelFace: View {
    @Environment(\.theme) private var theme
    @Environment(\.chatMode) private var mode
    @Environment(\.chatFactsShort) private var short
    @Environment(\.chatModelOpen) private var open
    let models: AgentModels?
    let model: String
    let effort: String?
    /// The face is the key of a menu that changes the model.
    let opens: Bool

    public init(models: AgentModels?, model: String, effort: String?, opens: Bool = false) {
        self.models = models
        self.model = model
        self.effort = effort
        self.opens = opens
    }

    public var title: String {
        Self.name(of: model, in: models)
    }

    public var body: some View {
        let levels = models?.efforts(for: model) ?? []
        HStack(spacing: 6) {
            Text(verbatim: title).lineLimit(1)
            if let effort {
                if let level = levels.firstIndex(where: { $0.id == effort }) {
                    EffortBars(level: level, count: levels.count, height: 10, width: 2)
                        .help(Text(verbatim: levels[level].label))
                } else {
                    Text(verbatim: effort)
                }
            }
            if let mode {
                Text(verbatim: "·").foregroundStyle(theme.colors.inkFaint).accessibilityHidden(true)
                ChatModeLabel(mode: mode, named: !short)
            }
            if opens {
                Image(systemName: "chevron.down").font(scaled: 8, weight: .bold)
                    .foregroundStyle(open ? theme.colors.accentStrong : theme.colors.inkFaint)
                    .rotationEffect(.degrees(open ? 180 : 0))
                    .accessibilityHidden(true)
            }
        }
        .appTextStyle(.callout)
        .fontWeight(.medium)
    }

    /// Maps a full model ID (e.g. `claude-sonnet-4-5-20250929`) to a display name by matching
    /// a known short ID (e.g. `sonnet`) and joining 1-2 digit version segments (skipping dates).
    static func name(of model: String, in models: AgentModels?) -> String {
        guard let models else { return model }
        if let known = models.models.first(where: { $0.id == model }) {
            return known.label
        }
        let parts = model.prefix { $0 != "[" }.split(separator: "-").map(String.init)
        for known in models.models {
            guard let at = parts.firstIndex(of: known.id) else { continue }
            let version = parts[(at + 1)...].prefix { $0.count <= 2 && $0.allSatisfy(\.isNumber) }
            return version.isEmpty ? known.label : "\(known.label) \(version.joined(separator: "."))"
        }
        return model
    }
}
