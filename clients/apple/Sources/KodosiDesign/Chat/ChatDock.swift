import KodosiBridge
import SwiftUI

/// What sits under the conversation: the agent's list of its work, the waiting messages, the
/// question of the agent, the composer and the facts of the session.
struct ChatDock<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    @Bindable var chat: TerminalChat
    let agent: AgentKind
    var focused: FocusState<Bool>.Binding
    let modelKey: (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey
    let showTerminal: () -> Void
    let sent: () -> Void
    @State private var sends = 0
    @State private var selection: TextSelection?
    @State private var picked = 0
    @State private var modelOpen = false
    @State private var usageOpen = false

    private var typed: Bool {
        !chat.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var stops: Bool {
        chat.working && !typed
    }

    private var suggestions: [ChatCommand] {
        ChatCommandMatch.commands(chat.commands, for: chat.draft)
    }

    private var placeholder: String {
        chat.question != nil ? String(localized: "Or say what to do instead", bundle: .module)
            : String(localized: "Message \(agent.label)", bundle: .module)
    }

    var body: some View {
        let ask = chat.asks.first
        let away = chat.chat != nil && chat.state == .away
        let tasks = chat.tasks.contains { $0.state != .done } ? chat.tasks : []
        VStack(alignment: .leading, spacing: 8) {
            if !tasks.isEmpty {
                ChatTasks(tasks: tasks).transition(AnyTransition.rise)
            }
            ForEach(chat.queued) { queued in
                ChatQueuedRow(chat: chat, queued: queued, agent: agent, showTerminal: showTerminal).transition(AnyTransition.rise)
            }
            if let ask {
                // The card takes the room before the composer, which needs only its own line.
                ChatAskCard(chat: chat, ask: ask, agent: agent).id(ask.id).transition(AnyTransition.rise)
                    .layoutPriority(1)
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
                if !suggestions.isEmpty {
                    ChatCommandList(commands: suggestions, picked: min(picked, suggestions.count - 1), choose: choose)
                        .transition(AnyTransition.rise)
                }
                composer
                ChatFactsRow(facts: chat.facts, modelKey: modelKey, modelOpen: $modelOpen, usageOpen: $usageOpen)
                    .padding(.horizontal, 6)
            }
        }
        .padding(.horizontal, 10).padding(.top, 4).padding(.bottom, 8)
        .chatColumn()
        .animation(theme.motion.spring, value: tasks.isEmpty)
        .animation(theme.motion.spring, value: chat.queued.map(\.id))
        .animation(theme.motion.spring, value: ask?.id)
        .animation(theme.motion.fade, value: chat.state)
        .animation(theme.motion.fade, value: chat.failure)
        .animation(theme.motion.snappy, value: stops)
        .animation(theme.motion.snappy, value: typed)
        .animation(theme.motion.hover, value: focused.wrappedValue)
        .animation(theme.motion.snappy, value: suggestions.isEmpty)
        .onChange(of: chat.draft) { _, _ in picked = 0 }
        #if os(iOS)
            .sensoryFeedback(.impact(weight: .light), trigger: sends)
            // The keyboard of a phone would leave a new question little room.
            .onChange(of: ask?.id) { _, id in
                if id != nil, !typed {
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
                .appTextStyle(.body)
                .foregroundStyle(theme.colors.ink)
                .focused(focused)
                .padding(.leading, 10).padding(.vertical, 7)
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
        if let control = chat.control(of: chat.draft) {
            chat.draft = ""
            open(control)
            return
        }
        chat.submit()
        sends += 1
        sent()
    }

    private func open(_ control: ChatControl) {
        switch control {
        case .model: modelOpen = true
        case .usage: usageOpen = true
        }
    }

    private func move(_ step: Int) -> KeyPress.Result {
        let count = suggestions.count
        guard count > 0 else { return .ignored }
        picked = (min(picked, count - 1) + step + count) % count
        return .handled
    }

    /// Return and Tab take the picked command while the draft is a part of its name.
    private func complete() -> Bool {
        let suggestions = suggestions
        guard !suggestions.isEmpty else { return false }
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
            chat.draft = "/\(command.name) "
            focused.wrappedValue = true
        }
    }
}

/// A message that waits until the agent is free: my bubble, with the sign that it waits and a key
/// that takes it back.
private struct ChatQueuedRow: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let queued: ChatQueued
    let agent: AgentKind
    let showTerminal: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            Spacer(minLength: 40)
            if !queued.terminal {
                IconButton(title: "Remove", bundle: .module, symbol: "xmark",
                           identifier: "chat.queued.remove.\(AccessibilityIdentifier.token(queued.id))", size: 22)
                {
                    chat.unqueue(queued)
                }
            }
            VStack(alignment: .trailing, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: "clock").font(mac: 9, ios: .caption2, weight: .semibold).foregroundStyle(theme.colors.inkFaint)
                        .accessibilityLabel(Text("Waiting for \(agent.label)", bundle: .module))
                    Text(queued.text).appTextStyle(.footnote).foregroundStyle(theme.colors.ink).lineLimit(2)
                }
                .projectBubble()
                if let refused = queued.refused {
                    Text("The agent takes the next message in its terminal.", bundle: .module)
                        .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.caution)
                        .help(refused)
                } else if queued.terminal {
                    HStack(spacing: 8) {
                        Text("\(agent.label) shows this in its terminal.", bundle: .module)
                            .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                        Button(action: showTerminal) {
                            Text("Show the terminal", bundle: .module).appTextStyle(.caption).fontWeight(.semibold)
                                .foregroundStyle(theme.colors.accentStrong)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("chat.queued.terminal")
                    }
                }
            }
        }
        .accessibilityElement(children: .contain)
    }
}

/// The agent's list of its work while a step of it is open: how far the agent is and the step that
/// it works on, and the whole list at a press.
private struct ChatTasks: View {
    @Environment(\.theme) private var theme
    let tasks: [ChatTask]
    @State private var open = false

    var body: some View {
        let done = tasks.count { $0.state == .done }
        let current = tasks.first { $0.state == .working } ?? tasks.first { $0.state == .pending }
        VStack(alignment: .leading, spacing: 4) {
            Button { open.toggle() } label: {
                HStack(spacing: 7) {
                    Image(systemName: "checklist").font(mac: 10, ios: .caption, weight: .semibold).foregroundStyle(theme.colors.inkMuted)
                        .accessibilityHidden(true)
                    Text(verbatim: "\(done)/\(tasks.count)").font(mac: 11, ios: .caption, design: .monospaced).monospacedDigit()
                        .foregroundStyle(theme.colors.inkMuted)
                    ZStack(alignment: .leading) {
                        if !open, let current {
                            Text(verbatim: current.text).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                                .lineLimit(1).truncationMode(.tail)
                                .id(current.text)
                                .transition(AnyTransition.roll)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .clipped()
                    Image(systemName: "chevron.up").font(mac: 8, ios: .caption2, weight: .bold)
                        .rotationEffect(.degrees(open ? 180 : 0)).foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
                .controlHeight(22).touchTarget(Rectangle())
            }
            .buttonStyle(.plain)
            .help(open ? Text("Hide the tasks", bundle: .module) : Text("Show the tasks", bundle: .module))
            .accessibilityLabel(Text("Tasks: \(done) of \(tasks.count) done", bundle: .module))
            .accessibilityHint(open ? Text("Hide the tasks", bundle: .module) : Text("Show the tasks", bundle: .module))
            .accessibilityIdentifier("chat.tasks")
            if open {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(tasks.indices, id: \.self) { index in
                        ChatTaskLine(task: tasks[index])
                    }
                }
                .padding(.leading, 2).padding(.bottom, 4)
                .transition(AnyTransition.unfold)
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 2)
        .background { BezelBackground(shape: RoundedRectangle(cornerRadius: 13, style: .continuous), fill: theme.colors.raised, bezel: .quiet) }
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.snappy, value: current?.text)
        .animation(theme.motion.snappy, value: done)
    }
}

private struct ChatTaskLine: View {
    @Environment(\.theme) private var theme
    let task: ChatTask

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 7) {
            Group {
                switch task.state {
                case .done:
                    Image(systemName: "checkmark").foregroundStyle(theme.colors.ready)
                        .accessibilityLabel(Text("Done", bundle: .module))
                case .working:
                    Image(systemName: "circle.fill").foregroundStyle(theme.colors.ink)
                        .accessibilityHidden(true)
                case .pending:
                    Image(systemName: "circle").foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
            }
            .font(mac: 8, ios: .caption2, weight: .bold)
            .frame(width: 12)
            .contentTransition(.symbolEffect(.replace))
            Text(verbatim: task.text).appTextStyle(.footnote)
                .foregroundStyle(task.state == .working ? theme.colors.ink : theme.colors.inkMuted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .animation(theme.motion.snappy, value: task.state)
        .accessibilityElement(children: .combine)
    }
}

private struct ChatFactsRow<ModelKey: View>: View {
    @Environment(\.theme) private var theme
    let facts: ChatFacts
    let modelKey: (_ model: String, _ effort: String?, _ open: Binding<Bool>) -> ModelKey
    @Binding var modelOpen: Bool
    @Binding var usageOpen: Bool

    var body: some View {
        ViewThatFits(in: .horizontal) {
            line(full: true)
            VStack(alignment: .leading, spacing: 6) {
                model(full: true)
                HStack(spacing: 8) {
                    usage(full: true)
                    others
                }
            }
            .lineLimit(1)
            .fixedSize()
            line(full: false)
            ScrollView(.horizontal) { line(full: false) }.scrollIndicators(.hidden)
        }
    }

    private func line(full: Bool) -> some View {
        HStack(spacing: 8) {
            model(full: full)
            usage(full: full)
            others
        }
        .lineLimit(1)
        .fixedSize()
    }

    @ViewBuilder private func model(full: Bool) -> some View {
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

    @ViewBuilder private func usage(full: Bool) -> some View {
        if facts.context != nil || !facts.limits.isEmpty || (facts.cost ?? 0) > 0 {
            ChatUsageKey(facts: facts, open: $usageOpen, named: full)
        }
    }

    private var others: some View {
        Group {
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
        .tip(Text(verbatim: ChatWords.mode(mode)))
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
                        .monospacedDigit()
                        .foregroundStyle(limit.level == .normal ? theme.colors.ink : limit.level.color(theme.colors))
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(Text(verbatim: limit.spoken))
                }
                if facts.context == nil, facts.limits.isEmpty, let cost = facts.cost {
                    Text(verbatim: ChatWords.cost(cost)).monospacedDigit()
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
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
        .kodosiPopover(isPresented: $open, arrowEdge: .top, alignment: .leading) {
            ChatUsage(facts: facts)
        }
        .tip(Text("Usage", bundle: .module))
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
            .monospacedDigit()
            .foregroundStyle(level == .normal ? theme.colors.ink : level.color(theme.colors))
        }
        .animation(theme.motion.snappy, value: percent)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Context: \(percent)% used", bundle: .module))
    }
}

/// The agent's model by its name, and its effort as bars, as the key in a chat's facts shows them.
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
                        .tip(Text(verbatim: levels[level].label))
                } else {
                    Text(verbatim: effort)
                }
            }
            if let mode {
                Text(verbatim: "·").foregroundStyle(theme.colors.inkFaint).accessibilityHidden(true)
                ChatModeLabel(mode: mode, named: !short)
            }
            if opens {
                Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold))
                    .foregroundStyle(open ? theme.colors.accentStrong : theme.colors.inkFaint)
                    .rotationEffect(.degrees(open ? 180 : 0))
                    .accessibilityHidden(true)
            }
        }
        .appTextStyle(.callout)
        .fontWeight(.medium)
    }

    /// An agent reports its model by its full ID, such as claude-sonnet-5-5 or
    /// claude-sonnet-4-5-20250929, where the launcher knows the model by a shorter one, such as
    /// sonnet: the name of the shorter one and the version after it make "Sonnet 5.5".
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
