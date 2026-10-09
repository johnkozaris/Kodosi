import SwiftUI

struct SidebarRail: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let selection: SidebarSelection
    let pill: Namespace.ID

    var body: some View {
        VStack(spacing: 10) {
            Color.clear.frame(height: Metrics.titleBarHeight - 10)
            IconButton(title: "Show sidebar", symbol: "sidebar.left", identifier: "sidebar.toggle") {
                deps.workbench.sidebarCollapsed.toggle()
            }
            Button { deps.newSession() } label: {
                Image(systemName: "plus").font(.system(size: 14, weight: .bold)).foregroundStyle(theme.colors.onAccent)
                    .frame(width: 34, height: 34)
                    .background { RaisedBackground(shape: Circle(), fill: theme.colors.accent, elevation: .resting) }
            }
            .buttonStyle(PressScaleStyle())
            .help("New terminal").accessibilityLabel(Text("New terminal")).accessibilityIdentifier("sidebar.newSession")
            ScrollView {
                VStack(spacing: 6) {
                    ForEach(deps.missions) { mission in
                        railItem(selected: selection == .room(mission.id), help: mission.name,
                                 identifier: "missions.mission.\(AccessibilityIdentifier.token(mission.id))")
                        {
                            deps.workbench.section = .missions; deps.openMission(mission)
                        } content: {
                            RoomSigil(name: mission.name, key: mission.id, size: 26)
                                .overlay(alignment: .topTrailing) {
                                    if (deps.roomViews[mission.id]?.unread ?? 0) > 0 {
                                        Circle().fill(theme.colors.accent).frame(width: 8, height: 8)
                                            .overlay { Circle().stroke(theme.colors.ground, lineWidth: 1.5) }.offset(x: 3, y: -3)
                                    }
                                }
                        }
                    }
                    if !deps.missions.isEmpty, !deps.sessions.isEmpty {
                        Capsule().fill(theme.colors.hairline.opacity(0.7)).frame(width: 18, height: 1).padding(.vertical, 4)
                    }
                    ForEach(deps.sessions) { session in
                        railItem(selected: selection == .terminal(session.id), help: session.name,
                                 identifier: AccessibilityIdentifier.sidebarSession(session.id))
                        {
                            deps.activateSession(session.id)
                        } content: {
                            AgentMark(kind: session.agent, size: 26, activity: deps.activity(of: session))
                                .overlay(alignment: .topTrailing) {
                                    if deps.attention.contains(session.id) {
                                        BreathingDot(color: theme.colors.accent, size: 7).offset(x: 3, y: -3)
                                    }
                                }
                        }
                    }
                }
                .padding(.vertical, 6)
            }
            .scrollIndicators(.never)
            VStack(spacing: 6) {
                IconButton(title: "People", symbol: "person.2", identifier: "header.people", size: 32, active: selection == .people) {
                    deps.workbench.section = .people
                }
                IconButton(title: "Resume", symbol: "clock.arrow.circlepath", identifier: "sidebar.resume", size: 32) {
                    deps.workbench.showsHistory = true
                }
                IconButton(title: "Settings", symbol: "gearshape", identifier: "header.settings", size: 32, active: selection == .settings) {
                    deps.workbench.section = .settings
                }
            }
            .padding(.bottom, 12)
        }
        .frame(maxWidth: .infinity)
    }

    private func railItem(
        selected: Bool, help: String, identifier: String, action: @escaping () -> Void, @ViewBuilder content: () -> some View
    ) -> some View {
        Button(action: action) {
            content()
                .frame(width: 40, height: 40)
                .background {
                    if selected {
                        SelectionPill(namespace: pill, radius: Radius.lg)
                    }
                }
                .hoverRow(radius: Radius.lg, selected: selected)
        }
        .buttonStyle(PressScaleStyle())
        .help(help).accessibilityLabel(help).accessibilityIdentifier(identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
