import KodosiBridge
import KodosiDesign
import SwiftUI

enum SidebarSelection: Hashable {
    case room(String)
    case terminal(String)
    case rooms
    case people
    case settings
    case none
}

struct SidebarView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Namespace private var pill
    @State private var collapsedFolders: Set<String> = []

    private var selection: SidebarSelection {
        let workbench = deps.workbench
        switch workbench.section {
        case .sessions: return workbench.selectedSessionId.map(SidebarSelection.terminal) ?? .none
        case .missions: return workbench.selectedMissionId.map(SidebarSelection.room) ?? .rooms
        case .people: return .people
        case .settings: return .settings
        }
    }

    var body: some View {
        Group {
            if deps.workbench.sidebarCollapsed {
                SidebarRail(selection: selection, pill: pill)
            } else {
                expanded
            }
        }
        .animation(theme.motion.snappy, value: selection)
        .animation(theme.motion.spring, value: deps.sessions.map(\.id))
        .animation(theme.motion.spring, value: deps.missions.map(\.id))
        .animation(theme.motion.spring, value: deps.attention)
    }

    private var expanded: some View {
        VStack(spacing: 0) {
            HStack {
                Spacer()
                IconButton(title: "Hide sidebar", symbol: "sidebar.left", identifier: "sidebar.toggle") {
                    deps.workbench.sidebarCollapsed.toggle()
                }
            }
            .frame(height: Metrics.titleBarHeight).padding(.trailing, 8)
            HStack(spacing: 8) {
                KodosiWordmark(size: 16, blinks: false)
                Spacer()
                IconButton(title: "Go to…", symbol: "magnifyingglass", identifier: "sidebar.palette") {
                    deps.workbench.showsPalette = true
                }
            }
            .padding(.leading, 16).padding(.trailing, 8).padding(.top, 6)
            NewTerminalButton().padding(.horizontal, 12).padding(.top, 14).padding(.bottom, 10)
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    rooms
                    terminals
                }
                .padding(.horizontal, 8).padding(.vertical, 8)
            }
            .scrollIndicators(.never)
            footer
        }
    }

    private var rooms: some View {
        VStack(alignment: .leading, spacing: 2) {
            SidebarSectionHeader(title: "Rooms", addTitle: "New room", identifier: "missions.create", selected: selection == .rooms, pill: pill) {
                deps.showRooms()
            } add: {
                if deps.accountReady {
                    deps.workbench.showsNewRoom = true
                } else {
                    deps.beginSignIn()
                }
            }
            if !deps.accountReady {
                Button { deps.beginSignIn() } label: {
                    HStack(spacing: 9) {
                        Image(systemName: "person.2.wave.2").font(.system(size: 12, weight: .semibold)).frame(width: 22)
                        Text("Sign in to work together").appTextStyle(.footnote)
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(.horizontal, 8).frame(height: 32).hoverRow()
                }
                .buttonStyle(.plain).accessibilityIdentifier("sidebar.signIn.hint")
            }
            ForEach(deps.invitations) { invitation in
                SidebarInvitationRow(invitation: invitation).transition(AnyTransition.rise)
            }
            ForEach(deps.missions) { mission in
                SidebarRoomRow(mission: mission, selected: selection == .room(mission.id), pill: pill)
                    .transition(AnyTransition.rise)
            }
            if deps.missionListTruncated {
                Text("More rooms exist. Use Go to… to find them.").appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkFaint).padding(.horizontal, 10).padding(.top, 2)
                    .accessibilityLabel(Text("Some rooms are not shown"))
            }
        }
    }

    private var terminals: some View {
        VStack(alignment: .leading, spacing: 2) {
            SidebarSectionHeader(title: "Terminals", addTitle: "New terminal", identifier: "sidebar.terminals.new",
                                 selected: false, pill: pill)
            {
                deps.workbench.section = .sessions
            } add: {
                deps.openLauncher()
            }
            ForEach(SessionFolderGroup.groups(deps.sessions)) { group in
                let collapsed = collapsedFolders.contains(group.id)
                SidebarFolderHeader(group: group, collapsed: collapsed) {
                    withAnimation(theme.motion.spring) {
                        if collapsed {
                            collapsedFolders.remove(group.id)
                        } else {
                            collapsedFolders.insert(group.id)
                        }
                    }
                }
                if !collapsed {
                    ForEach(group.sessions) { session in
                        SidebarTerminalRow(session: session, selected: selection == .terminal(session.id), pill: pill)
                            .transition(AnyTransition.rise)
                    }
                }
            }
        }
    }

    private var footer: some View {
        VStack(spacing: 2) {
            SidebarNavRow(title: "People", symbol: "person.2", identifier: "header.people",
                          badge: deps.incomingRequests.count, selected: selection == .people, pill: pill)
            {
                deps.workbench.section = .people
            }
            if deps.updater.waiting {
                SidebarNavRow(title: "Restart to Update", symbol: "arrow.down.circle", identifier: "sidebar.update",
                              badge: 0, selected: false, pill: pill,
                              tint: deps.updater.critical ? theme.colors.caution : theme.colors.accentStrong)
                {
                    deps.updater.restart()
                }
            }
            AccountCapsule(selected: selection == .settings, pill: pill).padding(.top, 6)
        }
        .padding(.horizontal, 8).padding(.top, 8).padding(.bottom, 10)
    }
}

struct SelectionPill: View {
    @Environment(\.theme) private var theme
    let namespace: Namespace.ID
    var radius: CGFloat = Radius.md

    var body: some View {
        RaisedBackground(shape: RoundedRectangle(cornerRadius: radius, style: .continuous), fill: theme.colors.raised, elevation: .resting)
            .overlay(alignment: .leading) {
                Capsule().fill(theme.colors.accent).frame(width: 3, height: 14).padding(.leading, -1.5)
            }
            .matchedGeometryEffect(id: "sidebar.selection", in: namespace)
    }
}

struct NewTerminalButton: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        Button { deps.openLauncher() } label: {
            HStack(spacing: 8) {
                Image(systemName: "plus").font(.system(size: 12, weight: .bold))
                Text("New terminal")
                Spacer(minLength: 0)
                Text(verbatim: "⌘N").appTextStyle(.caption).opacity(0.6)
            }
            .frame(height: 34).contentShape(Rectangle())
        }
        .buttonStyle(.kodosi(.primary))
        .accessibilityIdentifier("sidebar.newSession")
    }
}

struct AccountCapsule: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let selected: Bool
    let pill: Namespace.ID

    var body: some View {
        HStack(spacing: 6) {
            if let prompt = deps.signInPrompt {
                signIn(prompt)
            } else {
                Button { deps.workbench.section = .settings } label: {
                    HStack(spacing: 9) {
                        PersonAvatar(name: deps.selfName, size: 24, isSelf: true)
                        Text(deps.selfName).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                        Spacer(minLength: 0)
                        Image(systemName: "gearshape").font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                    }
                    .padding(.horizontal, 8).frame(height: 38)
                    .background {
                        if selected {
                            SelectionPill(namespace: pill, radius: Radius.lg)
                        }
                    }
                    .hoverRow(radius: Radius.lg, selected: selected)
                }
                .buttonStyle(.plain).help("Settings").accessibilityLabel(Text("Settings"))
                .accessibilityIdentifier("header.settings")
            }
        }
    }

    private func signIn(_ prompt: String) -> some View {
        let approving = deps.userId != nil
        return HStack(spacing: 0) {
            Button { deps.beginSignIn() } label: {
                HStack(spacing: 9) {
                    Image(systemName: approving ? "checkmark.shield" : "person.fill")
                        .font(.system(size: 9, weight: .bold))
                        .frame(width: 20, height: 20)
                        .overlay {
                            Circle().strokeBorder(style: StrokeStyle(lineWidth: 1, dash: approving ? [] : [2.4, 2]))
                                .opacity(approving ? 0 : 0.55)
                        }
                    Text(prompt).appTextStyle(.subhead)
                    Spacer(minLength: 0)
                }
                .padding(.leading, 8).padding(.trailing, 6).frame(height: 34).contentShape(Rectangle())
            }
            .accessibilityIdentifier("header.signIn")
            Rectangle().fill(theme.colors.ink.opacity(0.12)).frame(width: 1, height: 18)
            Button { deps.workbench.section = .settings } label: {
                Image(systemName: "gearshape")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(selected ? theme.colors.accentStrong : theme.colors.inkMuted)
                    .frame(width: 38, height: 34).contentShape(Rectangle())
            }
            .help("Settings").accessibilityLabel(Text("Settings"))
            .accessibilityAddTraits(selected ? .isSelected : [])
            .accessibilityIdentifier("header.settings")
        }
        .buttonStyle(SplitPartStyle())
        .foregroundStyle(approving ? theme.colors.accentStrong : theme.colors.ink)
        .backgroundPreferenceValue(SplitPressed.self) { pressed in
            BezelBackground(
                shape: RoundedRectangle(cornerRadius: Radius.md, style: .continuous),
                fill: approving ? theme.colors.accentSoft.mix(with: theme.colors.accent, by: hovered ? 0.14 : 0)
                    : theme.colors.lifted.mix(with: theme.colors.ink, by: (theme.isDark ? 0.07 : 0) + (hovered ? 0.03 : 0)),
                bezel: .quiet
            )
            .environment(\.bezelPressed, pressed)
            .animation(theme.motion.snappy, value: pressed)
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
    }
}

private struct SplitPressed: PreferenceKey {
    static let defaultValue = false

    static func reduce(value: inout Bool, nextValue: () -> Bool) {
        value = value || nextValue()
    }
}

private struct SplitPartStyle: ButtonStyle {
    @Environment(\.theme) private var theme

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .preference(key: SplitPressed.self, value: configuration.isPressed)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(theme.motion.snappy, value: configuration.isPressed)
    }
}
