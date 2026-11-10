import KodosiBridge
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
                        topLeadingRadius: 14, bottomLeadingRadius: 14, bottomTrailingRadius: 4, topTrailingRadius: 14, style: .continuous,
                    )
                    .fill(theme.colors.accentSoft)
                }
        }
    }
}

/// Under a message that waits for its computer: which computer, and a key that takes it back.
struct DosiWaiting: View {
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let item: DosiItem
    let computer: DosiComputer

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 6) {
                line.lineLimit(1)
                key
            }
            VStack(alignment: .trailing, spacing: 2) {
                line.multilineTextAlignment(.trailing)
                key
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dosi.waiting.\(AccessibilityIdentifier.token(item.id))")
    }

    private var line: some View {
        HStack(alignment: .firstTextBaseline, spacing: 5) {
            Image(systemName: "clock").font(mac: 9, ios: .caption2, weight: .semibold).accessibilityHidden(true)
            Text("Waiting for \(computer.name)", bundle: .module)
        }
        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
        .accessibilityElement(children: .combine)
    }

    private var key: some View {
        Button { place.dosi.takeBack(item) } label: { Text("Do not send", bundle: .module) }
            .buttonStyle(.kodosi(.ghost, size: .small))
            .accessibilityIdentifier("dosi.waiting.remove.\(AccessibilityIdentifier.token(item.id))")
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
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let item: DosiItem
    let question: DosiQuestion

    var body: some View {
        Group {
            if let answer = question.answer {
                HStack(spacing: 7) {
                    if answer.isEmpty {
                        Image(systemName: "minus").font(mac: 9, ios: .caption2, weight: .bold).foregroundStyle(theme.colors.inkFaint)
                    } else {
                        Image(systemName: "checkmark").font(mac: 9, ios: .caption2, weight: .bold).foregroundStyle(theme.colors.ready)
                    }
                    Text(answered(answer)).font(mac: 11, ios: .caption, design: .monospaced).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                }
                .padding(.leading, 8).padding(.trailing, 10).controlHeight(22)
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
        let choice = answer.isEmpty ? String(localized: "not answered", bundle: .module) : answer.joined(separator: ", ")
        guard let reference = question.terminal else { return choice }
        return "\(choice) · \(place.terminal(reference)?.name ?? reference)"
    }
}

struct DosiQuestionCard: View {
    @Environment(DosiPlace.self) private var place
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
                if let reference = question.terminal, let session = place.terminal(reference) {
                    AgentMark(session: session, size: 18)
                }
                Text(question.question).font(mac: 12.5, ios: .subheadline, weight: .semibold).foregroundStyle(theme.colors.ink)
                    .textSelection(.enabled)
            }
            FlowLayout(spacing: 6) {
                ForEach(question.options, id: \.self) { option in
                    Button(option) { choose(option) }
                        .buttonStyle(.kodosi(question.multiple && picked.contains(option) ? .tinted : .secondary, size: .small))
                        .accessibilityIdentifier("dosi.answer.\(AccessibilityIdentifier.token(option)).\(AccessibilityIdentifier.token(item.id))")
                }
            }
            HStack(spacing: 6) {
                TextField(String(localized: "Something else", bundle: .module), text: $own)
                    .textFieldStyle(WellTextFieldStyle(radius: Radius.sm, compact: true))
                    .onSubmit(send)
                    .accessibilityIdentifier("dosi.answer.own.\(AccessibilityIdentifier.token(item.id))")
                if question.multiple || !words.isEmpty {
                    Button(action: send) { Text("Send", bundle: .module) }
                        .buttonStyle(.kodosi(.primary, size: .small)).disabled(choices.isEmpty)
                        .accessibilityIdentifier("dosi.answer.send.\(AccessibilityIdentifier.token(item.id))")
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
            place.answer(item, [option])
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
        place.answer(item, choices)
    }
}

struct DosiNoticeView: View {
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let notice: DosiNotice

    var body: some View {
        switch notice.tone {
        case .stopped:
            Text("Stopped", bundle: .module).font(mac: 10.5, ios: .caption2, design: .monospaced).foregroundStyle(theme.colors.inkFaint)
                .padding(.horizontal, 8).controlHeight(18).background(theme.colors.surface, in: Capsule())
        case .failed, .signIn:
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 8) {
                    Image(systemName: notice.tone == .signIn ? "key.fill" : "exclamationmark.triangle.fill")
                        .font(mac: 12, ios: .footnote, weight: .semibold).foregroundStyle(theme.colors.caution)
                    Text(notice.tone == .signIn ? "Dosi needs you to sign in" : "Dosi stopped", bundle: .module)
                        .font(mac: 12.5, ios: .subheadline, weight: .semibold).foregroundStyle(theme.colors.ink)
                }
                if let text = notice.text, !text.isEmpty {
                    Text(text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                }
                if notice.tone == .signIn, let openSettings = place.openSettings {
                    Button(action: openSettings) { Text("Open Settings", bundle: .module) }
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
            DeviceGlyph(label: computer).font(mac: 10, ios: .caption2, weight: .medium)
            Text("Working on \(computer)", bundle: .module)
        }
        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
        .padding(.leading, 28)
    }
}

struct DosiWelcome: View {
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let listening: Bool

    var body: some View {
        VStack(spacing: 14) {
            DosiFace(mood: place.dosi.mood(listening: listening), size: 58)
            Text("Your agent for your agents", bundle: .module).appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
                .multilineTextAlignment(.center)
            VStack(spacing: 6) {
                ForEach(Array(Self.asks.enumerated()), id: \.offset) { index, ask in
                    Button { place.send(ask) } label: {
                        Text(ask)
                        #if os(iOS)
                            .multilineTextAlignment(.center).lineLimit(3)
                        #endif
                    }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    #if os(iOS)
                        .fixedSize(horizontal: false, vertical: true)
                    #endif
                        .disabled(!place.canSend())
                        .arrive(delay: 0.2 + Double(index) * 0.06, rise: 6)
                }
            }
        }
        .padding(24)
        .arrive()
    }

    static let asks = [
        String(localized: "What needs me?", bundle: .module),
        String(localized: "Sum up my rooms", bundle: .module),
        String(localized: "Tell me when my terminals finish", bundle: .module),
    ]
}
