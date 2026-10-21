import KodosiDesign
import SwiftUI

enum PanelMenuIcon {
    case symbol(String)
    case agent(AgentKind)
}

struct PanelMenuRow<Trailing: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.closePanel) private var close
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false
    let title: Text
    let icon: PanelMenuIcon
    let identifier: String
    var destructive = false
    let action: () -> Void
    @ViewBuilder let trailing: () -> Trailing

    init(
        _ title: Text, icon: PanelMenuIcon, identifier: String, destructive: Bool = false, action: @escaping () -> Void,
        @ViewBuilder trailing: @escaping () -> Trailing,
    ) {
        self.title = title
        self.icon = icon
        self.identifier = identifier
        self.destructive = destructive
        self.action = action
        self.trailing = trailing
    }

    var body: some View {
        let lit = hovered && isEnabled
        Button {
            close()
            action()
        } label: {
            HStack(spacing: 9) {
                Group {
                    switch icon {
                    case let .symbol(symbol):
                        Image(systemName: symbol).font(.system(size: 12.5, weight: .semibold))
                            .foregroundStyle(iconColor(lit: lit))
                    case let .agent(kind):
                        AgentMark(kind: kind, size: 18, activity: .awake)
                    }
                }
                .frame(width: 18)
                title.font(.system(size: 12.5, weight: .medium)).foregroundStyle(titleColor)
                Spacer(minLength: 8)
                trailing()
            }
            .padding(.horizontal, 8).frame(height: 30)
            .background {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                    .fill(lit ? theme.colors.raised.mix(with: theme.colors.ink, by: 0.06) : .clear)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .accessibilityIdentifier(identifier)
    }

    private var titleColor: Color {
        if !isEnabled {
            return theme.colors.inkFaint
        }
        return destructive ? theme.colors.danger : theme.colors.ink
    }

    private func iconColor(lit: Bool) -> Color {
        if !isEnabled {
            return theme.colors.inkFaint
        }
        if destructive {
            return theme.colors.danger
        }
        return lit ? theme.colors.ink : theme.colors.inkMuted
    }
}

extension PanelMenuRow where Trailing == EmptyView {
    init(_ title: Text, icon: PanelMenuIcon, identifier: String, destructive: Bool = false, action: @escaping () -> Void) {
        self.init(title, icon: icon, identifier: identifier, destructive: destructive, action: action) { EmptyView() }
    }
}

struct PanelMenuDivider: View {
    @Environment(\.theme) private var theme

    var body: some View {
        Rectangle().fill(theme.colors.hairline).frame(height: 1).padding(.horizontal, 8).padding(.vertical, 3)
    }
}

struct PanelMenuHeader: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey

    var body: some View {
        Text(title).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            .padding(.horizontal, 10).padding(.top, 4).padding(.bottom, 2)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
