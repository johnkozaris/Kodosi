import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiPersonBubble: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        HStack {
            Spacer(minLength: 48)
            Text(RoomMessageText.formatted(text))
                .appTextStyle(.body).lineSpacing(2.5).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background {
                    UnevenRoundedRectangle(
                        topLeadingRadius: 14, bottomLeadingRadius: 14, bottomTrailingRadius: 4, topTrailingRadius: 14, style: .continuous
                    )
                    .fill(theme.colors.accentSoft)
                }
        }
    }
}

struct DosiProse: View {
    @Environment(\.theme) private var theme
    let text: String
    let live: Bool

    var body: some View {
        HStack(alignment: .lastTextBaseline, spacing: 3) {
            Text(RoomMessageText.formatted(text))
                .appTextStyle(.body).lineSpacing(3).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                .tint(theme.colors.accentStrong)
            if live {
                CursorBlock(width: 6, height: 14, blink: .typing).transition(.opacity)
            }
        }
        .animation(theme.motion.fade, value: live)
    }
}

struct DosiQuestionView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let item: DosiItem
    let question: DosiQuestion

    var body: some View {
        Group {
            if let answer = question.answer {
                HStack(spacing: 7) {
                    if answer.isEmpty {
                        Image(systemName: "minus").font(.system(size: 9, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
                    } else {
                        Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)).foregroundStyle(theme.colors.ready)
                    }
                    Text(answered(answer)).font(.system(size: 11, design: .monospaced)).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                }
                .padding(.leading, 8).padding(.trailing, 10).frame(height: 22)
                .background(theme.colors.surface, in: Capsule())
                .help(question.question)
                .transition(.opacity.combined(with: .scale(scale: 0.9, anchor: .leading)))
            } else {
                DosiQuestionCard(item: item, question: question).transition(AnyTransition.grow)
            }
        }
        .animation(theme.motion.spring, value: question.answer)
    }

    private func answered(_ answer: [String]) -> String {
        let choice = answer.isEmpty ? String(localized: "not answered") : answer.joined(separator: ", ")
        guard let reference = question.terminal else { return choice }
        return "\(choice) · \(deps.terminal(named: reference)?.name ?? reference)"
    }
}

struct DosiQuestionCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let item: DosiItem
    let question: DosiQuestion
    @State private var picked: Set<String> = []
    @State private var own = ""

    private var words: String {
        own.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var choices: [String] {
        question.options.filter(picked.contains) + (words.isEmpty ? [] : [words])
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(alignment: .center, spacing: 8) {
                if let reference = question.terminal, let session = deps.terminal(named: reference) {
                    AgentMark(session: session, size: 18)
                }
                Text(question.question).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(theme.colors.ink)
                    .textSelection(.enabled)
            }
            FlowLayout(spacing: 6) {
                ForEach(question.options, id: \.self) { option in
                    Button(option) { choose(option) }
                        .buttonStyle(.kodosi(question.multiple && picked.contains(option) ? .tinted : .secondary, size: .small))
                        .accessibilityIdentifier("dosi.answer.\(AccessibilityIdentifier.token(option))")
                }
            }
            HStack(spacing: 6) {
                TextField("Something else", text: $own)
                    .textFieldStyle(WellTextFieldStyle(radius: Radius.sm, compact: true))
                    .onSubmit(send)
                    .accessibilityIdentifier("dosi.answer.own")
                if question.multiple || !words.isEmpty {
                    Button("Send", action: send)
                        .buttonStyle(.kodosi(.primary, size: .small)).disabled(choices.isEmpty)
                        .accessibilityIdentifier("dosi.answer.send")
                        .transition(AnyTransition.pop)
                }
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.lg, fill: theme.colors.raised, elevation: .lifted)
        .animation(theme.motion.snappy, value: picked)
        .animation(theme.motion.snappy, value: words.isEmpty)
    }

    private func choose(_ option: String) {
        guard question.multiple else {
            deps.answerDosi(item, [option])
            return
        }
        if picked.contains(option) {
            picked.remove(option)
        } else {
            picked.insert(option)
        }
    }

    private func send() {
        guard !choices.isEmpty, question.multiple || choices.count == 1 else { return }
        deps.answerDosi(item, choices)
    }
}

struct DosiNoticeView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let notice: DosiNotice

    var body: some View {
        switch notice.tone {
        case .stopped:
            Text("Stopped").font(.system(size: 10.5, design: .monospaced)).foregroundStyle(theme.colors.inkFaint)
                .padding(.horizontal, 8).frame(height: 18).background(theme.colors.surface, in: Capsule())
        case .failed, .signIn:
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 8) {
                    Image(systemName: notice.tone == .signIn ? "key.fill" : "exclamationmark.triangle.fill")
                        .font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.caution)
                    Text(notice.tone == .signIn ? "Dosi needs you to sign in" : "Dosi stopped")
                        .font(.system(size: 12.5, weight: .semibold)).foregroundStyle(theme.colors.ink)
                }
                if let text = notice.text, !text.isEmpty {
                    Text(text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                }
                if notice.tone == .signIn {
                    Button("Open Settings") {
                        deps.workbench.settingsSection = .dosi
                        deps.workbench.section = .settings
                    }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 11)
            .frame(maxWidth: .infinity, alignment: .leading)
            .raised(Radius.lg, fill: theme.colors.raised, elevation: .resting)
            .transition(AnyTransition.grow)
        }
    }
}

struct DosiAway: View {
    @Environment(\.theme) private var theme
    let computer: String

    var body: some View {
        HStack(spacing: 7) {
            DeviceGlyph(label: computer).font(.system(size: 10, weight: .medium))
            Text("Working on \(computer)")
        }
        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
        .padding(.leading, 28)
    }
}

struct DosiWelcome: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let listening: Bool

    var body: some View {
        VStack(spacing: 14) {
            DosiFace(mood: deps.dosi.mood(listening: listening), size: 58)
            Text("Your agent for your agents").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
            VStack(spacing: 6) {
                ForEach(Array(Self.asks.enumerated()), id: \.offset) { index, ask in
                    Button(ask) { deps.sendToDosi(ask) }
                        .buttonStyle(.kodosi(.secondary, size: .small))
                        .disabled(deps.dosi.state == .unavailable)
                        .arrive(delay: 0.2 + Double(index) * 0.06, rise: 6)
                }
            }
        }
        .padding(24)
        .arrive()
    }

    static let asks = [
        String(localized: "What needs me?"),
        String(localized: "Sum up my rooms"),
        String(localized: "Tell me when my terminals finish"),
    ]
}
