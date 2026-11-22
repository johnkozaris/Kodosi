import KodosiBridge
import SwiftUI

/// A question of the agent, open by itself above the composer: the agent's mark, what it asks and
/// the status sign of a wait, then what the answer is about and the keys. The key that a person
/// pressed stays pressed until the question is gone, and the others wait.
struct ChatAskCard: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
    let agent: AgentKind

    var body: some View {
        Group {
            switch ask.body {
            case let .permission(tool, always, helper):
                ChatPermission(chat: chat, ask: ask, agent: agent, tool: tool, always: always, helper: helper)
            case let .question(questions):
                ChatQuestions(chat: chat, ask: ask, agent: agent, questions: questions)
            case let .plan(text, choices):
                ChatPlan(chat: chat, ask: ask, agent: agent, text: text, choices: choices)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.lg, fill: theme.colors.raised, elevation: .lifted)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat.ask.\(AccessibilityIdentifier.token(ask.id))")
    }
}

/// The first row of a question card: who asks, about what, and the sign that it waits for the
/// person. The hand waves once when the card comes.
private struct AskHeader<Title: View>: View {
    let agent: AgentKind
    let sign: StatusSign.Form
    @ContentBuilder let title: Title

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            AgentMark(kind: agent, size: 20).accessibilityHidden(true)
            title
            Spacer(minLength: 4)
            StatusSign(form: sign, size: 14)
        }
        .frame(minHeight: 22)
    }
}

/// A key of a question: while a person's answer is on its way, the key of that answer stays
/// down and the other keys wait.
private struct AnswerKey: View {
    let chat: TerminalChat
    let ask: ChatAsk
    let answer: ChatAnswer
    let kind: KodosiButtonStyle.Kind
    let label: Text
    var wide = false

    var body: some View {
        let given = chat.answers[ask.id]
        Button { chat.answer(ask, answer) } label: { label.truncationMode(.middle).frame(maxWidth: wide ? .infinity : nil) }
            .buttonStyle(.kodosi(kind, size: .small))
            .disabled(given != nil && given != answer)
            .allowsHitTesting(given == nil)
            .scaleEffect(given == answer ? 0.97 : 1)
            .accessibilityAddTraits(given == answer ? .isSelected : [])
        #if os(iOS)
            .sensoryFeedback(.impact(weight: .light), trigger: given == answer)
        #endif
    }
}

private struct ChatPermission: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
    let agent: AgentKind
    let tool: ChatTool
    let always: AskAlways?
    let helper: String

    /// A command that is longer than its line shows whole below the line: a person reads all of
    /// what they allow.
    private var whole: String? {
        if case let .command(command, _, _)? = tool.detail, command != tool.target || command.count > 96 {
            return command
        }
        return nil
    }

    /// Above a phone's keyboard the command and the change get less room, so that the keys stay.
    var body: some View {
        ViewThatFits(in: .vertical) {
            card(command: 140, lines: 14)
            card(command: 56, lines: 4)
            card(command: 0, lines: 0)
        }
    }

    private func card(command: CGFloat, lines most: Int) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            AskHeader(agent: agent, sign: .hand) {
                HStack(spacing: 6) {
                    StepSymbol(kind: tool.kind)
                    Text(verbatim: tool.target.isEmpty ? tool.name : tool.target)
                        .appTextStyle(.mono).foregroundStyle(theme.colors.ink)
                        .lineLimit(whole == nil || command == 0 ? 3 : 1).truncationMode(.middle)
                        .textSelection(.enabled)
                        .help(tool.name)
                }
            }
            if !helper.isEmpty {
                Label { Text(verbatim: helper) } icon: { Image(systemName: "person.2") }
                    .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
            }
            if let whole, command > 0 {
                CodeTile(text: whole, most: command)
            } else if whole == nil, case let .change(_, _, _, lines)? = tool.detail, !lines.isEmpty, most > 0 {
                ChangeLines(lines: lines, most: most)
            }
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 6) {
                    decisions
                    Spacer(minLength: 6)
                    alwaysKey
                }
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) { decisions }
                    alwaysKey
                }
            }
        }
    }

    @ContentBuilder
    private var decisions: some View {
        AnswerKey(chat: chat, ask: ask, answer: .allow, kind: .primary, label: Text("Allow", bundle: .module))
            .accessibilityIdentifier("chat.ask.allow")
        AnswerKey(chat: chat, ask: ask, answer: .deny(nil), kind: .secondary, label: Text("Deny", bundle: .module))
            .accessibilityIdentifier("chat.ask.deny")
    }

    @ContentBuilder
    private var alwaysKey: some View {
        if let offer {
            let offered = Text(verbatim: offer).monospaced(Self.code(always)).foregroundStyle(theme.colors.inkMuted)
            AnswerKey(chat: chat, ask: ask, answer: .always, kind: .secondary, label: Text("Always \(offered)", bundle: .module))
                .help(offer)
                .accessibilityIdentifier("chat.ask.always")
        }
    }

    private var offer: String? {
        switch always {
        case let .rule(_, text): text
        case let .mode(mode): ChatWords.mode(mode)
        case let .folder(name): name
        case nil: nil
        }
    }

    /// A rule and a folder say themselves in the letters of code, and a mode in words.
    private static func code(_ always: AskAlways?) -> Bool {
        if case .mode? = always {
            return false
        }
        return true
    }
}

private struct ChatQuestions: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
    let agent: AgentKind
    let questions: [ChatQuestion]
    @State private var picked: [Int: [String]] = [:]

    /// One question with one choice goes at the first press.
    private var direct: Bool {
        questions.count == 1 && !questions[0].multiple
    }

    private var sent: [[String]]? {
        if case let .answers(answers)? = chat.answers[ask.id] {
            return answers
        }
        return nil
    }

    var body: some View {
        ViewThatFits(in: .vertical) {
            VStack(alignment: .leading, spacing: 10) {
                header
                list
                sendKey
            }
            VStack(alignment: .leading, spacing: 10) {
                header
                ScrollView { VStack(alignment: .leading, spacing: 14) { list } }
                    .frame(maxHeight: 260).fixedSize(horizontal: false, vertical: true)
                sendKey
            }
        }
    }

    private var header: some View {
        AskHeader(agent: agent, sign: .question) {
            let topics = questions.map(\.header).filter { !$0.isEmpty }
            if !topics.isEmpty {
                Text(verbatim: topics.joined(separator: " · ")).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
            }
        }
    }

    @ContentBuilder
    private var list: some View {
        let sent = sent
        ForEach(questions.indices, id: \.self) { index in
            let question = questions[index]
            VStack(alignment: .leading, spacing: 8) {
                Text(question.question).appTextStyle(.body).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                ChatOptions(question: question, chosen: sent.map { index < $0.count ? $0[index] : [] } ?? picked[index] ?? [],
                            sent: sent != nil) { choose($0, in: index) }
            }
        }
    }

    @ContentBuilder
    private var sendKey: some View {
        if !direct {
            Button { send() } label: { Text("Send", bundle: .module) }
                .buttonStyle(.kodosi(.primary, size: .small))
                .disabled(sent != nil || questions.indices.contains { (picked[$0] ?? []).isEmpty })
                .accessibilityIdentifier("chat.ask.send")
        }
    }

    private func choose(_ label: String, in index: Int) {
        if direct {
            chat.answer(ask, .answers([[label]]))
            return
        }
        var chosen = picked[index] ?? []
        if questions[index].multiple {
            if let at = chosen.firstIndex(of: label) {
                chosen.remove(at: at)
            } else {
                chosen.append(label)
            }
        } else {
            chosen = [label]
        }
        withAnimation(theme.motion.snappy) { picked[index] = chosen }
    }

    private func send() {
        chat.answer(ask, .answers(questions.indices.map { picked[$0] ?? [] }))
    }
}

private struct ChatOptions: View {
    @Environment(\.theme) private var theme
    let question: ChatQuestion
    let chosen: [String]
    let sent: Bool
    let choose: (String) -> Void
    @Namespace private var thumb

    var body: some View {
        VStack(spacing: 2) {
            ForEach(question.options, id: \.label) { option in
                let on = chosen.contains(option.label)
                Button { choose(option.label) } label: {
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verbatim: option.label).appTextStyle(.subhead)
                                .foregroundStyle(on ? theme.colors.ink : theme.colors.inkMuted)
                            if !option.description.isEmpty {
                                Text(verbatim: option.description).appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                                    .lineLimit(3)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .multilineTextAlignment(.leading)
                        if question.multiple {
                            Image(systemName: on ? "checkmark.circle.fill" : "circle")
                                .font(mac: 13, ios: .body)
                                .foregroundStyle(on ? theme.colors.accent : theme.colors.inkFaint)
                                .contentTransition(.symbolEffect(.replace))
                                .accessibilityHidden(true)
                        }
                    }
                    .padding(.horizontal, 11).padding(.vertical, 7)
                    .frame(minHeight: Self.row)
                    .background {
                        if on {
                            RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.md - 3, style: .continuous),
                                             fill: theme.colors.raised, elevation: .resting)
                                .matchedGeometryEffect(id: question.multiple ? option.label : "thumb", in: thumb)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(PressScaleStyle(scale: 0.98))
                .disabled(sent && !on)
                .allowsHitTesting(!sent)
                .accessibilityAddTraits(on ? .isSelected : [])
                .accessibilityIdentifier("chat.ask.option.\(AccessibilityIdentifier.token(option.label))")
            }
        }
        .padding(3)
        .well(Radius.md)
    }

    private static var row: CGFloat {
        #if os(iOS)
            40
        #else
            28
        #endif
    }
}

private struct ChatPlan: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
    let agent: AgentKind
    let text: String
    let choices: [PlanChoice]

    /// Above a phone's keyboard the plan gets less room, and at last only its keys stay.
    var body: some View {
        ViewThatFits(in: .vertical) {
            card(most: 260)
            card(most: 120)
            card(most: 0)
        }
    }

    private func card(most: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            AskHeader(agent: agent, sign: .hand) {
                HStack(spacing: 6) {
                    StepSymbol(kind: .plan)
                    Text(verbatim: ChatWords.mode(.plan)).appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                }
            }
            if !text.isEmpty, most > 0 {
                ScrollView {
                    AgentProse(text: text, live: false).frame(maxWidth: .infinity, alignment: .leading).padding(12)
                }
                .frame(maxHeight: most).fixedSize(horizontal: false, vertical: true)
                .background(theme.colors.surface, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
            }
            keys
        }
    }

    private var keys: some View {
        #if os(iOS)
            VStack(spacing: 6) { choiceKeys(wide: true) }
        #else
            FlowLayout(spacing: 6) { choiceKeys(wide: false) }
        #endif
    }

    @ContentBuilder
    private func choiceKeys(wide: Bool) -> some View {
        ForEach(choices.indices, id: \.self) { index in
            let choice = choices[index]
            AnswerKey(chat: chat, ask: ask, answer: .plan(choice), kind: index == 0 ? .primary : .secondary, label: Self.label(choice), wide: wide)
                .accessibilityIdentifier("chat.ask.plan.\(choice.rawValue)")
        }
        AnswerKey(chat: chat, ask: ask, answer: .deny(nil), kind: .secondary, label: Text("Keep planning", bundle: .module), wide: wide)
            .accessibilityIdentifier("chat.ask.deny")
    }

    private static func label(_ choice: PlanChoice) -> Text {
        switch choice {
        case .go: Text("Approve", bundle: .module)
        case .ask: Text("Approve, and ask before each change", bundle: .module)
        case .edits: Text("Approve, and allow its edits", bundle: .module)
        case .auto: Text("Approve, and use the safety check", bundle: .module)
        case .alone: Text("Approve, and ask nothing", bundle: .module)
        case .helpers: Text("Approve, and let helpers work alone", bundle: .module)
        case .leave: Text("Leave plan mode", bundle: .module)
        }
    }
}
