import KodosiBridge
import KodosiDesign
import SwiftUI

struct UsageMenuRow: View {
    @Environment(\.theme) private var theme
    let launcher: LauncherState
    @Binding var open: Bool
    @State private var hovered = false

    private var nearest: UsageLimit? {
        UsageLimit.nearest(of: launcher.usageAccounts.flatMap { launcher.usage(of: $0.info.agent, account: $0.account.name)?.limits ?? [] })
    }

    var body: some View {
        let lit = hovered || open
        Button { open.toggle() } label: {
            HStack(spacing: 9) {
                UsageRing(limit: nearest, lineWidth: 1.75, loading: launcher.loadingUsage)
                    .frame(width: 14, height: 14)
                    .frame(width: 18)
                Text("Provider Usage").font(.system(size: 12.5, weight: .medium)).foregroundStyle(theme.colors.ink)
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
        .accessibilityLabel(Text("Provider Usage"))
        .accessibilityValue(Text(verbatim: nearest.map { String(localized: "Up to \($0.used)% used") }
                ?? (launcher.loadingUsage ? String(localized: "Loading") : "")))
        .accessibilityAddTraits(open ? .isSelected : [])
        .accessibilityIdentifier("sidebar.usage")
    }
}

struct UsagePanel: View {
    @Environment(\.theme) private var theme
    let launcher: LauncherState
    let start: (AgentInfo, AgentAccount) -> Void

    private static let dial: CGFloat = 26
    private static let column: CGFloat = 44

    private struct Section: Identifiable {
        let info: AgentInfo
        var rows: [(account: AgentAccount, usage: AccountUsage?)] = []

        var id: String {
            info.agent
        }

        var windows: [UsageLimit] {
            UsagePanel.windows(of: rows.compactMap(\.usage))
        }
    }

    private var sections: [Section] {
        var sections: [Section] = []
        for (info, account) in launcher.usageAccounts {
            let usage = launcher.usage(of: info.agent, account: account.name)
            guard usage != nil || !launcher.usageRead else { continue }
            if sections.last?.info.agent != info.agent {
                sections.append(Section(info: info))
            }
            sections[sections.count - 1].rows.append((account, usage))
        }
        return sections
    }

    static func windows(of usages: [AccountUsage]) -> [UsageLimit] {
        var windows: [UsageLimit] = []
        for limit in usages.flatMap(\.limits) where !windows.contains(where: { $0.id == limit.id }) {
            windows.append(limit)
        }
        return windows.sorted { ($0.hours ?? .max, $0.scope ?? "") < ($1.hours ?? .max, $1.scope ?? "") }
    }

    var body: some View {
        let sections = sections
        VStack(alignment: .leading, spacing: 2) {
            if sections.isEmpty {
                Text("No plan limits to show")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                    .padding(.horizontal, 8).frame(height: 30)
            }
            ForEach(sections.enumerated(), id: \.element.id) { index, section in
                if index > 0 {
                    Rectangle().fill(theme.colors.hairline).frame(height: 1).padding(.horizontal, 8).padding(.vertical, 4)
                }
                let windows = section.windows
                header(section, windows: windows)
                ForEach(section.rows, id: \.account.name) { row in
                    UsageAccountRow(
                        info: section.info, account: row.account, usage: row.usage, windows: windows, dial: Self.dial,
                        column: Self.column, loading: row.usage == nil && launcher.loadingUsage,
                    ) {
                        start(section.info, row.account)
                    }
                }
            }
        }
        .padding(6)
        .frame(width: 296)
        .animation(theme.motion.spring, value: sections.map(\.id))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Provider Usage"))
        .accessibilityIdentifier("sidebar.usage.panel")
    }

    private func header(_ section: Section, windows: [UsageLimit]) -> some View {
        HStack(spacing: 8) {
            AgentMark(kind: section.info.kind, size: 15, activity: .awake)
            Text(verbatim: section.info.kind.label).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            Spacer(minLength: 8)
            HStack(spacing: 0) {
                ForEach(windows) { limit in
                    Text(verbatim: limit.window)
                        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                        .lineLimit(1).minimumScaleFactor(0.8)
                        .frame(width: Self.column)
                }
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
    let column: CGFloat
    let loading: Bool
    let start: () -> Void
    @State private var hovered = false

    var body: some View {
        let title = account.title(program: info.program)
        Button(action: start) {
            HStack(spacing: 9) {
                AccountMark(
                    initial: String(title.prefix(1)), key: account.signIn, signedIn: true, size: 20,
                    tint: info.kind.tint(account: info.accounts.firstIndex { $0.name == account.name }),
                )
                Text(verbatim: title)
                    .font(.system(size: 12.5, weight: .medium)).foregroundStyle(theme.colors.ink)
                    .lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 10)
                HStack(spacing: 0) {
                    ForEach(windows) { window in
                        let limit = usage?.limits.first { $0.id == window.id }
                        VStack(spacing: 2) {
                            if usage == nil || limit != nil {
                                UsageDial(limit: limit, size: dial, loading: loading)
                            } else {
                                Color.clear.frame(width: dial, height: dial)
                            }
                            UsageReset(limit: limit)
                        }
                        .frame(width: column)
                    }
                    if windows.isEmpty {
                        UsageDial(limit: nil, size: dial, loading: loading).frame(width: column)
                    }
                }
            }
            .opacity(usage?.stale == true ? 0.55 : 1)
            .padding(.horizontal, 8).frame(height: windows.isEmpty ? 38 : 50)
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
        .accessibilityLabel(Text(verbatim: ([title, info.kind.label] + (usage?.limits.map(\.spokenWithReset) ?? [])).joined(separator: ", ")))
        .accessibilityHint(Text("Opens the launcher with this account"))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { start() }
        .accessibilityIdentifier("sidebar.usage.\(info.agent).\(AccessibilityIdentifier.token(account.name))")
    }
}
