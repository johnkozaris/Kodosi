import KodosiDesign
import SwiftUI

/// The menu row that opens the usage panel beside the menu. Its small ring shows the account that is
/// nearest to a limit.
struct UsageMenuRow: View {
    @Environment(\.theme) private var theme
    let launcher: LauncherState
    @Binding var open: Bool
    @State private var hovered = false

    private var nearest: UsageLimit? {
        launcher.usageAccounts.compactMap { launcher.usage(of: $0.info.agent, account: $0.account.name)?.headline }
            .max { $0.used < $1.used }
    }

    var body: some View {
        let lit = hovered || open
        Button { open.toggle() } label: {
            HStack(spacing: 9) {
                UsageRing(limit: nearest, lineWidth: 1.75)
                    .frame(width: 14, height: 14)
                    .frame(width: 18)
                Text("Usage").font(.system(size: 12.5, weight: .medium)).foregroundStyle(theme.colors.ink)
                Spacer(minLength: 8)
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(lit ? theme.colors.inkMuted : theme.colors.inkFaint)
            }
            .padding(.horizontal, 8).frame(height: 30)
            .background {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                    .fill(lit ? theme.colors.raised.mix(with: theme.colors.ink, by: 0.06) : .clear)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { inside in
            hovered = inside
            if inside {
                open = true
            }
        }
        .animation(theme.motion.hover, value: lit)
        .accessibilityLabel(Text("Usage"))
        .accessibilityValue(Text(verbatim: nearest.map { String(localized: "Up to \($0.used)% used") } ?? ""))
        .accessibilityAddTraits(open ? .isSelected : [])
        .accessibilityIdentifier("sidebar.usage")
    }
}

/// How much of each signed-in account's plan limits is used, one dial for each limit. A click on an
/// account starts a terminal with it.
struct UsagePanel: View {
    @Environment(\.theme) private var theme
    let launcher: LauncherState
    let start: (AgentInfo, AgentAccount) -> Void

    private static let dial: CGFloat = 26

    private struct Section: Identifiable {
        let info: AgentInfo
        var rows: [(account: AgentAccount, usage: AccountUsage?)] = []
        var windows: [UsageLimit] = []

        var id: String {
            info.agent
        }
    }

    /// One section for each agent. Until the first answer, every signed-in account waits with a dashed
    /// ring; after it, only the accounts with plan limits stay.
    private var sections: [Section] {
        var sections: [Section] = []
        for (info, account) in launcher.usageAccounts {
            let usage = launcher.usage(of: info.agent, account: account.name)
            guard usage != nil || !launcher.usageRead else { continue }
            if sections.last?.info.agent != info.agent {
                sections.append(Section(info: info))
            }
            let last = sections.count - 1
            sections[last].rows.append((account, usage))
            for limit in usage?.limits ?? [] where !sections[last].windows.contains(where: { $0.id == limit.id }) {
                sections[last].windows.append(limit)
            }
        }
        return sections
    }

    var body: some View {
        let sections = sections
        VStack(alignment: .leading, spacing: 2) {
            if sections.isEmpty {
                Text("No plan limits to show")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                    .padding(.horizontal, 8).frame(height: 30)
            }
            ForEach(Array(sections.enumerated()), id: \.element.id) { index, section in
                if index > 0 {
                    Rectangle().fill(theme.colors.hairline).frame(height: 1).padding(.horizontal, 8).padding(.vertical, 4)
                }
                header(section)
                ForEach(section.rows, id: \.account.name) { row in
                    UsageAccountRow(
                        info: section.info, account: row.account, usage: row.usage, windows: section.windows, dial: Self.dial,
                    ) {
                        start(section.info, row.account)
                    }
                }
            }
        }
        .padding(6)
        .frame(width: 272)
        .animation(theme.motion.spring, value: sections.map(\.id))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Usage"))
        .accessibilityIdentifier("sidebar.usage.panel")
    }

    private func header(_ section: Section) -> some View {
        HStack(spacing: 8) {
            AgentMark(kind: section.info.kind, size: 15, activity: .awake)
            Text(verbatim: section.info.kind.label).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            Spacer(minLength: 8)
            ForEach(section.windows) { limit in
                Text(verbatim: limit.window)
                    .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                    .lineLimit(1).minimumScaleFactor(0.8)
                    .frame(width: Self.dial + 6)
            }
        }
        .padding(.horizontal, 8).frame(height: 24)
        .accessibilityElement(children: .combine)
    }
}

private struct UsageAccountRow: View {
    @Environment(\.theme) private var theme
    let info: AgentInfo
    let account: AgentAccount
    let usage: AccountUsage?
    let windows: [UsageLimit]
    let dial: CGFloat
    let start: () -> Void
    @State private var hovered = false

    var body: some View {
        let title = account.title(program: info.program)
        Button(action: start) {
            HStack(spacing: 9) {
                AccountMark(initial: String(title.prefix(1)), key: account.signIn, signedIn: true, size: 20)
                Text(verbatim: title)
                    .font(.system(size: 12.5, weight: .medium)).foregroundStyle(theme.colors.ink)
                    .lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 10)
                ForEach(windows) { window in
                    Group {
                        if let usage {
                            if let limit = usage.limits.first(where: { $0.id == window.id }) {
                                UsageDial(limit: limit, size: dial)
                            } else {
                                Color.clear.frame(width: dial, height: dial)
                            }
                        } else {
                            UsageDial(limit: nil, size: dial)
                        }
                    }
                    .frame(width: dial + 6)
                }
                if windows.isEmpty {
                    UsageDial(limit: nil, size: dial).frame(width: dial + 6)
                }
            }
            .opacity(usage?.stale == true ? 0.55 : 1)
            .padding(.horizontal, 8).frame(height: 38)
            .background {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                    .fill(hovered ? theme.colors.raised.mix(with: theme.colors.ink, by: 0.06) : .clear)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .tip(verbatim: account.identity ?? title)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: ([title, info.kind.label] + (usage?.limits.map(\.spoken) ?? [])).joined(separator: ", ")))
        .accessibilityHint(Text("Starts a terminal with this account"))
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("sidebar.usage.\(info.agent).\(AccessibilityIdentifier.token(account.name))")
    }
}
