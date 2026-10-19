import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

struct SidebarMenu: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var open = false
    @State private var hovered = false
    let selected: Bool
    var size: CGFloat = 34
    var alignment: HorizontalAlignment = .trailing

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        let tinted = open || selected
        Button { open.toggle() } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(tinted ? theme.colors.accentStrong : hovered ? theme.colors.ink : theme.colors.inkMuted)
                .frame(width: size, height: size)
                .background {
                    BezelBackground(shape: shape, fill: tinted ? theme.colors.accentSoft : theme.buttonFace(lit: hovered), bezel: .quiet)
                }
                .contentShape(shape)
        }
        .buttonStyle(PressScaleStyle())
        .overlay(alignment: .topTrailing) {
            if !deps.people.incoming.isEmpty {
                CountBadge(count: deps.people.incoming.count).offset(x: 5, y: -5)
                    .allowsHitTesting(false)
                    .transition(AnyTransition.pop)
            }
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: tinted)
        .animation(theme.motion.snappy, value: deps.people.incoming.count)
        .tip("Settings and more")
        .accessibilityLabel(Text("Settings and more"))
        .accessibilityIdentifier("sidebar.menu")
        .kodosiPopover(isPresented: $open, arrowEdge: .top, alignment: alignment) { SidebarMenuList() }
    }
}

private struct SidebarMenuList: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            SidebarMenuRow(title: "Settings", symbol: "gearshape", identifier: "header.settings") {
                deps.workbench.section = .settings
            } trailing: {
                Text(verbatim: "⌘,").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            }
            SidebarMenuRow(title: "Friends", symbol: "person.2", identifier: "header.people") {
                deps.workbench.section = .people
            } trailing: {
                if !deps.people.incoming.isEmpty {
                    CountBadge(count: deps.people.incoming.count)
                }
            }
            Rectangle().fill(theme.colors.hairline).frame(height: 1).padding(.horizontal, 8).padding(.vertical, 3)
            SidebarMenuRow(title: "About Kodosi", symbol: "info.circle", identifier: "sidebar.about") {
                NSApp.orderFrontStandardAboutPanel(nil)
            } trailing: {
                EmptyView()
            }
            SidebarMenuRow(title: "Check for Updates…", symbol: "arrow.down.circle", identifier: "sidebar.updates") {
                deps.updater.check()
            } trailing: {
                EmptyView()
            }
            .disabled(!deps.updater.configured)
        }
        .padding(6)
        .frame(width: 224)
    }
}

private struct SidebarMenuRow<Trailing: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.closePanel) private var close
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false
    let title: LocalizedStringKey
    let symbol: String
    let identifier: String
    let action: () -> Void
    @ViewBuilder let trailing: () -> Trailing

    var body: some View {
        let lit = hovered && isEnabled
        Button {
            close()
            action()
        } label: {
            HStack(spacing: 9) {
                Image(systemName: symbol).font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(lit ? theme.colors.ink : theme.colors.inkMuted)
                    .frame(width: 18)
                Text(title).font(.system(size: 12.5, weight: .medium)).foregroundStyle(theme.colors.ink)
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
        .opacity(isEnabled ? 1 : 0.45)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .accessibilityIdentifier(identifier)
    }
}
