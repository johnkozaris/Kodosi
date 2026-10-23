import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

struct SidebarMenu: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var open = false
    @State private var hovered = false
    @State private var signingOut = false
    let selected: Bool
    var size: CGFloat = 34
    var alignment: HorizontalAlignment = .trailing

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        let lit = hovered || open || selected
        Button { open.toggle() } label: {
            SettingsGlyph(size: size * 0.55)
                .foregroundStyle(lit ? theme.colors.ink : theme.colors.inkMuted)
                .frame(width: size, height: size)
                .background { BezelBackground(shape: shape, fill: theme.buttonFace(lit: lit), bezel: .quiet) }
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
        .animation(theme.motion.hover, value: lit)
        .animation(theme.motion.snappy, value: deps.people.incoming.count)
        .tip("Settings and more")
        .accessibilityLabel(Text("Settings and more"))
        .accessibilityIdentifier("sidebar.menu")
        .kodosiPopover(isPresented: $open, arrowEdge: .top, alignment: alignment) {
            SidebarMenuList { signingOut = true }
        }
        .onChange(of: open) { _, open in
            guard open else { return }
            Task {
                await deps.launcher.refresh()
                await deps.launcher.refreshUsage()
            }
        }
        .confirmationDialog("Sign out?", isPresented: $signingOut, titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { deps.account.signOut() }
        } message: {
            Text("Sharing ends. Terminals on this Mac keep running.")
        }
    }
}

private struct SidebarMenuList: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.closePanel) private var close
    @State private var usage = false
    let signOut: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            PanelMenuRow(Text("Settings"), icon: .symbol("gearshape"), identifier: "header.settings") {
                deps.workbench.section = .settings
            } trailing: {
                Text(verbatim: "⌘,").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            }
            .closesFlyout($usage)
            PanelMenuRow(Text("Friends"), icon: .symbol("person.2"), identifier: "header.people") {
                deps.workbench.section = .people
            } trailing: {
                if !deps.people.incoming.isEmpty {
                    CountBadge(count: deps.people.incoming.count)
                }
            }
            .closesFlyout($usage)
            if !deps.launcher.usageAccounts.isEmpty {
                UsageMenuRow(launcher: deps.launcher, open: $usage)
                    .kodosiFlyout(isPresented: usage) {
                        UsagePanel(launcher: deps.launcher, start: start)
                    }
                    .transition(.opacity)
            }
            PanelMenuDivider()
            PanelMenuHeader(title: "Resume")
            ForEach(Provider.allCases, id: \.self) { provider in
                PanelMenuRow(Text(verbatim: provider.agent.label), icon: .agent(provider.agent),
                             identifier: "sidebar.resume.\(provider.rawValue)")
                {
                    deps.workbench.resume(provider)
                }
                .closesFlyout($usage)
            }
            PanelMenuDivider()
            PanelMenuRow(Text("About Kodosi"), icon: .symbol("info.circle"), identifier: "sidebar.about") {
                NSApp.orderFrontStandardAboutPanel(nil)
            }
            .closesFlyout($usage)
            PanelMenuRow(Text("Check for Updates…"), icon: .symbol("arrow.down.circle"), identifier: "sidebar.updates") {
                deps.updater.check()
            }
            .disabled(!deps.updater.configured)
            .closesFlyout($usage)
            if deps.account.userId != nil {
                PanelMenuDivider()
                PanelMenuRow(Text("Sign out…"), icon: .symbol("rectangle.portrait.and.arrow.right"), identifier: "sidebar.signOut",
                             action: signOut)
                    .closesFlyout($usage)
            }
        }
        .padding(6)
        .frame(width: 224)
        .animation(theme.motion.fade, value: deps.launcher.usageAccounts.isEmpty)
    }

    private func start(_ info: AgentInfo, _ account: AgentAccount) {
        close()
        if deps.settings.lastComputer != nil {
            deps.settings.lastComputer = nil
            deps.settings.save()
        }
        var pick = deps.launcher.choice.pick(info.agent)
        pick.account = account.name
        deps.launcher.choose(.tab(info.agent))
        deps.launcher.choose(.pick(pick))
        deps.openLauncher()
    }
}

private extension View {
    func closesFlyout(_ flyout: Binding<Bool>) -> some View {
        onHover { inside in
            if inside {
                flyout.wrappedValue = false
            }
        }
    }
}

private struct SettingsGlyph: View {
    let size: CGFloat

    var body: some View {
        Sliders()
            .stroke(style: StrokeStyle(lineWidth: size / 12, lineCap: .round))
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

private struct Sliders: Shape {
    func path(in rect: CGRect) -> Path {
        let unit = min(rect.width, rect.height) / 24
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * unit, y: rect.minY + y * unit)
        }
        var path = Path()
        for (start, end, y) in [(4.0, 10.0, 7.0), (14, 20, 7), (4, 7, 17), (11, 20, 17)] {
            path.move(to: point(start, y))
            path.addLine(to: point(end, y))
        }
        for (x, y) in [(12.0, 7.0), (9, 17)] {
            path.addEllipse(in: CGRect(origin: point(x - 2, y - 2), size: CGSize(width: 4 * unit, height: 4 * unit)))
        }
        return path
    }
}
