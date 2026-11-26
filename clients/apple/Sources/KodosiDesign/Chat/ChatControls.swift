import KodosiBridge
import SwiftUI

/// The agent's slash commands that start as a person types after a slash, then the ones that hold
/// the typed letters.
enum ChatCommandMatch {
    static func commands(_ commands: [ChatCommand], for draft: String) -> [ChatCommand] {
        guard draft.hasPrefix("/"), !draft.contains(where: \.isWhitespace) else { return [] }
        let typed = draft.dropFirst().lowercased()
        let starting = commands.filter { $0.name.lowercased().hasPrefix(typed) }
        guard !typed.isEmpty else { return starting }
        return starting + commands.filter { !$0.name.lowercased().hasPrefix(typed) && $0.name.lowercased().contains(typed) }
    }
}

/// The commands that fit the draft, above the composer, as the agent's own menu shows them in
/// its terminal.
struct ChatCommandList: View {
    @Environment(\.theme) private var theme
    let commands: [ChatCommand]
    let picked: Int
    let choose: (ChatCommand) -> Void

    private static let rowHeight: CGFloat = 30

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(commands.enumerated()), id: \.element.id) { index, command in
                        row(command, picked: index == picked).id(command.id)
                    }
                }
                .padding(4)
            }
            .scrollIndicators(.automatic)
            .frame(maxHeight: Self.rowHeight * 6.5)
            .fixedSize(horizontal: false, vertical: true)
            .onChange(of: picked) { _, picked in
                if commands.indices.contains(picked) {
                    proxy.scrollTo(commands[picked].id)
                }
            }
        }
        .raised(Radius.lg, fill: theme.colors.raised)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Commands", bundle: .module))
        .accessibilityIdentifier("chat.commands")
    }

    private func row(_ command: ChatCommand, picked: Bool) -> some View {
        Button { choose(command) } label: {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(verbatim: "/\(command.name)").appTextStyle(.mono).foregroundStyle(theme.colors.ink)
                    .lineLimit(1).fixedSize()
                if command.terminal {
                    Tag(text: String(localized: "Command needs terminal view", bundle: .module), symbol: "apple.terminal", tone: .accent)
                        .accessibilityHint(Text(verbatim: command.description))
                } else {
                    Text(verbatim: command.description).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .lineLimit(1).truncationMode(.tail)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .frame(minHeight: Self.rowHeight)
            .background {
                if picked {
                    RoundedRectangle(cornerRadius: Radius.sm, style: .continuous).fill(theme.colors.ink.opacity(0.07))
                }
            }
            .touchTarget(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("chat.command.\(command.name)")
    }
}

/// What the agent reports of its use: the limits of its plan with their resets, its context and
/// the cost of the conversation.
struct ChatUsage: View {
    @Environment(\.theme) private var theme
    let facts: ChatFacts

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let context = facts.context {
                let percent = Int((context.used * 100).rounded())
                line {
                    UsageRing(used: percent, level: ContextMeter.level(context), lineWidth: 2.5).frame(width: 20, height: 20)
                } title: {
                    Text("Context: \(percent)% used", bundle: .module)
                } detail: {
                    Text("\(Self.count(context.tokens)) of \(Self.count(context.window)) tokens", bundle: .module)
                }
            }
            ForEach(facts.limits) { limit in
                line {
                    UsageDial(limit: limit, size: 22)
                } title: {
                    Text(verbatim: limit.spoken)
                } detail: {
                    if let left = ResetTime.left(limit) {
                        Text("Resets in \(ResetTime.spoken(left))", bundle: .module)
                    }
                }
            }
            if let cost = facts.cost, cost > 0 {
                line {
                    Image(systemName: "dollarsign.circle").font(scaled: 18, weight: .regular)
                        .foregroundStyle(theme.colors.inkMuted).frame(width: 22)
                } title: {
                    Text("This conversation", bundle: .module)
                } detail: {
                    Text(verbatim: ChatWords.cost(cost)).monospacedDigit()
                }
            }
        }
        .padding(14)
        .frame(minWidth: 240, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("chat.usage")
    }

    private func line(
        @ContentBuilder sign: () -> some View, @ContentBuilder title: () -> some View, @ContentBuilder detail: () -> some View,
    ) -> some View {
        HStack(alignment: .center, spacing: 10) {
            sign().accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                title().appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                detail().appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            }
        }
    }

    private static func count(_ tokens: UInt64) -> String {
        Int(clamping: tokens).formatted(.number.notation(.compactName))
    }
}
