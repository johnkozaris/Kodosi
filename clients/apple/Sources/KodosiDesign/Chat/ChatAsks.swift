import KodosiBridge
import SwiftUI

/// A question of the agent, open by itself above the composer. The button that a person pressed
/// stays pressed until the question is gone, and the others wait.
struct ChatAskCard: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            switch ask.body {
            case let .permission(tool, always, helper):
                ChatPermission(chat: chat, ask: ask, tool: tool, always: always, helper: helper)
            case let .question(questions):
                ChatQuestions(chat: chat, ask: ask, questions: questions)
            case let .plan(text, choices):
                ChatPlan(chat: chat, ask: ask, text: text, choices: choices)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.lg, fill: theme.colors.raised, elevation: .lifted)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat.ask.\(AccessibilityIdentifier.token(ask.id))")
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

    var body: some View {
        let given = chat.answers[ask.id]
        Button { chat.answer(ask, answer) } label: { label.truncationMode(.middle) }
            .buttonStyle(.kodosi(kind, size: .small))
            .disabled(given != nil && given != answer)
            .allowsHitTesting(given == nil)
            .scaleEffect(given == answer ? 0.97 : 1)
            .accessibilityAddTraits(given == answer ? .isSelected : [])
    }
}

private struct ChatPermission: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
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

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            StatusSign(form: .hand, size: 12)
            StepGlyph(symbol: ChatWords.symbol(tool.kind))
            Text(verbatim: tool.target.isEmpty ? tool.name : tool.target)
                .appTextStyle(.mono).foregroundStyle(theme.colors.ink).lineLimit(whole == nil ? 3 : 1).truncationMode(.middle)
                .textSelection(.enabled)
                .help(tool.name)
        }
        if !helper.isEmpty {
            Label { Text(verbatim: helper) } icon: { Image(systemName: "person.2") }
                .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
        }
        if let whole {
            ScrollView {
                Text(verbatim: whole).appTextStyle(.monoCaption).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading).padding(8)
            }
            .frame(maxHeight: 140).fixedSize(horizontal: false, vertical: true)
            .well(Radius.md)
        } else if case let .change(_, _, _, lines)? = tool.detail, !lines.isEmpty {
            ChangeLines(lines: lines, most: 14)
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

    @ContentBuilder
    private var decisions: some View {
        AnswerKey(chat: chat, ask: ask, answer: .deny(nil), kind: .secondary, label: Text("Deny", bundle: .module))
            .accessibilityIdentifier("chat.ask.deny")
        AnswerKey(chat: chat, ask: ask, answer: .allow, kind: .primary, label: Text("Allow", bundle: .module))
            .accessibilityIdentifier("chat.ask.allow")
    }

    @ContentBuilder
    private var alwaysKey: some View {
        if let offer {
            let offered = Text(verbatim: offer).monospaced().foregroundStyle(theme.colors.inkMuted)
            AnswerKey(chat: chat, ask: ask, answer: .always, kind: .secondary, label: Text("Always \(offered)", bundle: .module))
                .help(offer)
                .accessibilityIdentifier("chat.ask.always")
        }
    }

    private var offer: String? {
        switch always {
        case let .rule(_, text): text
        case let .mode(mode): ChatWords.mode(mode)
        case let .folder(path): HostPath.folderName(path)
        case nil: nil
        }
    }
}

private struct ChatQuestions: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
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
        let sent = sent
        ForEach(questions.indices, id: \.self) { index in
            let question = questions[index]
            let chosen = sent.map { index < $0.count ? $0[index] : [] } ?? picked[index] ?? []
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    if index == 0 {
                        StatusSign(form: .question, size: 12)
                    }
                    if !question.header.isEmpty {
                        Text(verbatim: question.header).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                            .padding(.horizontal, 7).controlHeight(18).background(theme.colors.surface, in: Capsule())
                    }
                    Text(question.question).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                }
                FlowLayout(spacing: 6) {
                    ForEach(question.options, id: \.label) { option in
                        let on = chosen.contains(option.label)
                        Button(option.label) { choose(option.label, in: index) }
                            .buttonStyle(.kodosi(on ? .tinted : .secondary, size: .small))
                            .disabled(sent != nil && !on)
                            .allowsHitTesting(sent == nil)
                            .help(option.description)
                            .accessibilityHint(Text(verbatim: option.description))
                            .accessibilityAddTraits(on ? .isSelected : [])
                    }
                }
            }
        }
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

private struct ChatPlan: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let ask: ChatAsk
    let text: String
    let choices: [PlanChoice]

    var body: some View {
        HStack(spacing: 8) {
            StatusSign(form: .hand, size: 12)
            StepGlyph(symbol: ChatWords.symbol(.plan))
        }
        ScrollView {
            AgentProse(text: text, live: false).frame(maxWidth: .infinity, alignment: .leading).padding(10)
        }
        .frame(maxHeight: 260).fixedSize(horizontal: false, vertical: true)
        .well(Radius.md)
        FlowLayout(spacing: 6) {
            ForEach(choices.indices, id: \.self) { index in
                let choice = choices[index]
                AnswerKey(chat: chat, ask: ask, answer: .plan(choice), kind: index == 0 ? .primary : .secondary, label: Self.label(choice))
                    .accessibilityIdentifier("chat.ask.plan.\(choice.rawValue)")
            }
            AnswerKey(chat: chat, ask: ask, answer: .deny(nil), kind: .secondary, label: Text("Deny", bundle: .module))
                .accessibilityIdentifier("chat.ask.deny")
        }
    }

    private static func label(_ choice: PlanChoice) -> Text {
        switch choice {
        case .go: Text("Approve", bundle: .module)
        case .ask: Text("Approve, and ask before each change", bundle: .module)
        case .edits: Text("Approve, and allow its edits", bundle: .module)
        case .alone: Text("Approve, and ask nothing", bundle: .module)
        case .helpers: Text("Approve, and let helpers work alone", bundle: .module)
        case .leave: Text("Leave plan mode", bundle: .module)
        }
    }
}
