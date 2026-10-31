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
    @Namespace private var fold
    @State private var frames = FoldFrames()
    @State private var pointer = SidebarPointer()
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
        let workbench = deps.workbench
        ZStack(alignment: .topLeading) {
            if workbench.sidebarCollapsed {
                SidebarRail(selection: selection, pill: pill).frame(width: Metrics.sidebarRailWidth).leaving(theme)
            } else {
                VStack(spacing: 0) {
                    HStack {
                        Spacer()
                        IconButton(title: "Hide sidebar", symbol: "sidebar.left", identifier: "sidebar.toggle") {
                            workbench.sidebarCollapsed.toggle()
                        }
                    }
                    .frame(height: Metrics.titleBarHeight).padding(.trailing, 8)
                    header
                    if workbench.dosiOpen {
                        DosiPanel().frame(width: workbench.dosiWidth).leaving(theme)
                    } else {
                        expanded.frame(width: workbench.sidebarWidth).leaving(theme)
                    }
                }
                .frame(width: workbench.dosiOpen ? workbench.dosiWidth : workbench.sidebarWidth, alignment: .leading)
                .leaving(theme)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
        .overlay(alignment: .topLeading) {
            if !theme.motion.reduced {
                FoldFlights(place: workbench.foldPlace)
            }
        }
        .coordinateSpace(.named(FoldFrames.space))
        .onContinuousHover(coordinateSpace: .named(FoldFrames.space)) { phase in
            switch phase {
            case let .active(location): pointer.location = location
            case .ended: pointer.location = nil
            }
        }
        .environment(\.sidebarFold, fold)
        .environment(\.sidebarFrames, frames)
        .environment(\.sidebarPointer, pointer)
        .environment(\.sidebarFolding, workbench.folding && !theme.motion.reduced)
        .animation(theme.motion.soft, value: workbench.dosiOpen)
        .animation(theme.motion.soft, value: workbench.sidebarCollapsed)
        .animation(theme.motion.snappy, value: selection)
        .animation(theme.motion.spring, value: deps.listedSessions.map(\.id))
        .animation(theme.motion.spring, value: deps.missions.map(\.id))
        .animation(theme.motion.spring, value: deps.listedSessions.map(\.attention))
    }

    private var header: some View {
        HStack(spacing: 8) {
            if deps.workbench.dosiOpen {
                DosiBackButton().transition(Self.back(theme))
            }
            KodosiWordmark(size: 16, blinks: false)
            Spacer(minLength: 8)
            HStack(spacing: 2) {
                if deps.workbench.dosiOpen, !deps.dosi.items.isEmpty {
                    DosiNewChatButton().transition(Self.back(theme))
                }
                IconButton(title: "Go to…", symbol: "magnifyingglass", identifier: "sidebar.palette") {
                    deps.workbench.showsPalette = true
                }
            }
        }
        .frame(height: 30)
        .padding(.leading, deps.workbench.dosiOpen ? 10 : 16).padding(.trailing, 8).padding(.top, 4)
        .arrive(delay: 0.15, rise: 0)
    }

    private static func back(_ theme: AppTheme) -> AnyTransition {
        guard !theme.motion.reduced else { return .identity }
        return .asymmetric(
            insertion: AnyTransition.pop.animation(theme.motion.snappy?.delay(0.12)),
            removal: AnyTransition.pop.animation(theme.motion.fade),
        )
    }

    private var expanded: some View {
        VStack(spacing: 0) {
            NewTerminalButton().padding(.horizontal, 12).padding(.top, 12).padding(.bottom, 6)
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    rooms
                    terminals
                }
                .padding(.horizontal, 8).padding(.vertical, 8)
            }
            .scrollIndicators(.never)
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(FoldFrames.space)) } action: { frames.views[.row] = $0 }
            .arrive(delay: 0.22, rise: 0)
            if deps.workbench.dosiOn {
                SidebarDosiRow().padding(.horizontal, 8).padding(.top, 6)
            }
            footer.arrive(delay: 0.28, rise: 0)
        }
    }

    private var rooms: some View {
        VStack(alignment: .leading, spacing: 2) {
            SidebarSectionHeader(title: "Rooms", addTitle: "New room", identifier: "missions.create") {
                deps.showRooms()
            } add: {
                if deps.account.isReady {
                    deps.workbench.showsNewRoom = true
                } else {
                    deps.beginSignIn()
                }
            }
            if !deps.account.isReady {
                Button { deps.beginSignIn() } label: {
                    HStack(spacing: 9) {
                        Image(systemName: "person.2.wave.2").font(.system(size: 12, weight: .semibold)).frame(width: 22)
                        Text(deps.account.userId == nil
                            ? String(localized: "Sign in to work together") : String(localized: "Trust this Mac to work together"))
                            .appTextStyle(.footnote)
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
            SidebarSectionHeader(title: "Terminals", addTitle: "New terminal", identifier: "sidebar.terminals.new") {
                deps.workbench.section = .sessions
            } add: {
                deps.openLauncher()
            }
            let listed = deps.listedSessions
            let pinned = deps.workbench.pinnedSessionIds.compactMap { id in listed.first { $0.id == id } }
            if !pinned.isEmpty {
                SidebarPinnedHeader()
                ForEach(pinned) { session in
                    SidebarTerminalRow(session: session, selected: selection == .terminal(session.id), pill: pill)
                        .transition(AnyTransition.rise)
                }
            }
            let computers = SessionComputerGroup.groups(listed.filter { !deps.workbench.isPinned($0.id) })
            let titled = computers.contains { $0.computer != nil }
            ForEach(computers) { computer in
                if titled {
                    SidebarComputerHeader(group: computer)
                }
                ForEach(computer.folders) { group in
                    folder(group)
                }
            }
        }
    }

    @ViewBuilder
    private func folder(_ group: SessionFolderGroup) -> some View {
        let collapsed = group.directory != nil && collapsedFolders.contains(group.id)
        if group.directory != nil {
            SidebarFolderHeader(group: group, collapsed: collapsed) {
                withAnimation(theme.motion.spring) {
                    if collapsed {
                        collapsedFolders.remove(group.id)
                    } else {
                        collapsedFolders.insert(group.id)
                    }
                }
            }
            .background(alignment: .leading) {
                if collapsed {
                    ZStack {
                        ForEach(group.sessions) { session in
                            Color.clear.frame(width: 22, height: 22).fold(.terminal(session.id), at: .row)
                        }
                    }
                    .padding(.leading, 4)
                }
            }
        }
        if !collapsed {
            ForEach(group.sessions) { session in
                SidebarTerminalRow(session: session, selected: selection == .terminal(session.id), pill: pill, showsOwner: false)
                    .transition(AnyTransition.rise)
            }
        }
    }

    private var footer: some View {
        VStack(spacing: 2) {
            if deps.updater.waiting {
                SidebarNavRow(title: "Restart to Update", symbol: "arrow.down.circle", identifier: "sidebar.update",
                              badge: 0, selected: false, pill: pill,
                              tint: deps.updater.critical ? theme.colors.caution : theme.colors.accentStrong)
                {
                    deps.updater.restart()
                }
            }
            HStack(spacing: 6) {
                AccountCapsule()
                SidebarMenu(selected: selection == .people || selection == .settings)
            }
            .padding(.top, 6)
        }
        .padding(.horizontal, 8).padding(.top, 2).padding(.bottom, 10)
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
                Text(verbatim: deps.settings.shortcuts.keys(for: .newTerminal).first?.label ?? "")
                    .appTextStyle(.caption).opacity(0.6)
            }
            .frame(height: 34).contentShape(Rectangle())
        }
        .buttonStyle(.kodosi(.primary))
        .fold(FoldPart.newTerminal)
        .accessibilityIdentifier("sidebar.newSession")
    }
}

struct AccountCapsule: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        if let prompt = deps.signInPrompt {
            let approving = deps.account.userId != nil
            Button { deps.beginSignIn() } label: {
                HStack(spacing: 9) {
                    Image(systemName: approving ? "checkmark.shield" : "person.fill")
                        .font(.system(size: 9, weight: .bold))
                        .frame(width: 20, height: 20)
                        .overlay {
                            Circle().strokeBorder(style: StrokeStyle(lineWidth: 1, dash: approving ? [] : [2.4, 2]))
                                .opacity(approving ? 0 : 0.55)
                        }
                    Text(prompt)
                    Spacer(minLength: 0)
                    if approving {
                        StatusSign(form: .hand)
                    }
                }
                .padding(.horizontal, -6).frame(height: 34)
            }
            .buttonStyle(.kodosi(.secondary))
            .accessibilityIdentifier("header.signIn")
        } else {
            Button { deps.workbench.showAccount() } label: {
                HStack(spacing: 9) {
                    PersonAvatar(name: deps.account.selfName, size: 24, isSelf: true)
                    Text(deps.account.selfName).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    Spacer(minLength: 0)
                    if !deps.account.deviceRequests.isEmpty {
                        StatusSign(form: .hand).transition(AnyTransition.pop)
                    }
                }
                .padding(.horizontal, 5).frame(height: 34)
                .hoverRow()
                .animation(theme.motion.snappy, value: deps.account.deviceRequests.isEmpty)
            }
            .buttonStyle(PressScaleStyle(scale: 0.98))
            .tip("Account").accessibilityLabel(Text("Account"))
            .accessibilityIdentifier("sidebar.account")
        }
    }
}
