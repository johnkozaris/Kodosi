import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomContentView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @Bindable var state: RoomViewState

    var body: some View {
        Group {
            if state.shownSection == 0 {
                RoomTerminals(roomId: roomId, state: state)
            } else if deps.rooms[roomId] == nil {
                if let failure = state.failure {
                    if !state.conversationVisible {
                        RoomRecovery(message: failure) { Task { await deps.readRoom(roomId) } }
                    }
                } else {
                    RoomSkeleton()
                }
            } else if state.shownSection == 1 {
                RoomTasksView(roomId: roomId, state: state)
            } else {
                RoomRepositoriesView(roomId: roomId, state: state)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .id(state.shownSection)
        .transition(.asymmetric(
            insertion: state.shownSection == 0 || theme.motion.reduced ? .identity : .opacity.combined(with: .offset(y: 6)),
            removal: .identity,
        ))
    }
}

private struct RoomTerminals: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Namespace private var tab
    @Namespace private var keys
    let roomId: String
    @Bindable var state: RoomViewState
    @State private var adding = false
    @State private var tabFrame: CGRect?
    @State private var stageFrame = CGRect.zero

    private static let space = "room.terminals"

    private var sessions: [RuntimeSession] {
        deps.roomSessions(roomId)
    }

    private var mine: [RuntimeSession] {
        deps.sessions.filter { $0.isOwner && $0.kind == .local && $0.missionId != roomId }
    }

    var body: some View {
        let sessions = sessions
        let current = sessions.first { $0.id == state.selectedTerminal && deps.workbench.stagedSessionIds.contains($0.id) }
        VStack(spacing: 0) {
            if !sessions.isEmpty {
                HStack(spacing: 0) {
                    ViewThatFits(in: .horizontal) {
                        tabs(sessions, current: current)
                        ScrollView(.horizontal) { tabs(sessions, current: current) }.scrollIndicators(.never)
                    }
                    .wellCapsule()
                    .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 8).padding(.bottom, 8)
                .animation(theme.motion.spring, value: sessions.map(\.id))
            }
            if let current {
                pair(current)
            } else if sessions.isEmpty {
                EmptyState(title: "No terminals here yet", message: "Add one. Everyone in the room gets full control.") {
                    TerminalGhost()
                } actions: {
                    Button { deps.openLauncher(roomId: roomId) } label: { Label("New terminal", systemImage: "plus") }
                        .buttonStyle(.kodosi(.primary)).accessibilityIdentifier("room.terminal.new")
                    if !mine.isEmpty {
                        Menu {
                            ForEach(mine) { session in Button(session.name) { share(session) } }
                        } label: { Text("Share one of mine") }
                            .menuStyle(.button).buttonStyle(.kodosi(.secondary)).fixedSize()
                            .accessibilityIdentifier("room.terminal.share")
                    }
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        Text("Open a terminal").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 230, maximum: 320), spacing: 12)], alignment: .leading, spacing: 12) {
                            ForEach(sessions) { session in
                                TerminalCard(session: session, rested: session.canOpen ? .awake : .asleep) {
                                    open(session)
                                } detail: {
                                    HStack(spacing: 4) {
                                        DeviceGlyph(label: session.hostLabel).font(.system(size: 9, weight: .medium))
                                        SessionActivityText(session: session, fallback: session.hostLabel)
                                    }
                                } accessory: {
                                    if let sign = deps.sign(of: session) {
                                        StatusSign(form: sign, size: 13).transition(AnyTransition.pop)
                                    }
                                    if !session.isOwner, let owner = session.ownerName {
                                        PersonAvatar(name: owner, key: session.ownerUserId, size: 20)
                                    }
                                }
                                .contextMenu { removeButton(session) }
                                .accessibilityIdentifier("room.terminal.card.\(AccessibilityIdentifier.token(session.id))")
                            }
                        }
                    }
                    .padding(24).frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .coordinateSpace(.named(Self.space))
    }

    /// The terminal, with its canvas beside it when it is open, flowing out of its tab.
    private func pair(_ current: RuntimeSession) -> some View {
        CanvasStage(canvas: state.showsCanvas && current.canvas != nil ? current : nil, wide: $state.canvasWide,
                    width: $state.canvasWidth, tab: tabFrame?.offsetBy(dx: -stageFrame.minX, dy: -stageFrame.minY), always: true)
        {
            state.showsCanvas = false
            state.canvasWide = false
        } terminal: {
            SessionTileView(session: current, isFocused: false, roomEmbedded: true)
                .environment(\.terminalStageVisible, deps.workbench.section == .missions)
                .transformEnvironment(\.terminalInteractionEnabled) { $0 = $0 && interactive }
                .id(current.id)
                .transition(.identity)
                .terminalScope()
        }
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(Self.space)) } action: { stageFrame = $0 }
        .padding([.horizontal, .bottom], 6)
        .environment(\.tileRadius, Radius.xl)
        .environment(\.canvasPlace, CanvasPlace(flight: keys, open: state.showsCanvas && current.canvas != nil ? current.id : nil) { session, section in
            if let section {
                deps.canvas.show(section, in: session)
            }
            state.selectedTerminal = session.id
            state.showsCanvas = true
        })
    }

    private var interactive: Bool {
        let workbench = deps.workbench
        return workbench.section == .missions && workbench.detailsSessionId == nil && workbench.sharingSessionId == nil
            && !workbench.showsPalette && !workbench.showsHistory && !workbench.showsNewRoom
    }

    private func tabs(_ sessions: [RuntimeSession], current: RuntimeSession?) -> some View {
        HStack(spacing: 2) {
            ForEach(sessions) { session in
                tabChip(session, selected: current?.id == session.id).transition(AnyTransition.pop)
            }
            addMenu
        }
        .padding(3)
    }

    private func tabChip(_ session: RuntimeSession, selected: Bool) -> some View {
        Button { open(session) } label: {
            TabLabel(session: session, selected: selected)
                .modifier(TabScope(terminal: selected))
                .background {
                    if selected {
                        TabPill()
                            .terminalScope()
                            .matchedGeometryEffect(id: "tab", in: tab)
                            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(Self.space)) } action: { frame in
                                withAnimation(tabFrame == nil ? nil : theme.motion.snappy) { tabFrame = frame }
                            }
                    }
                }
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help(session.folderName.map { String(localized: "\($0) on \(session.hostLabel)") } ?? session.hostLabel)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("room.terminal.\(AccessibilityIdentifier.token(session.id))")
        .contextMenu {
            Button("Details") { deps.workbench.detailsSessionId = session.id }
            if session.canControl, session.canOpen {
                Button("Open Browser") {
                    open(session)
                    state.showsCanvas = true
                    deps.openBrowser(of: session)
                }
            }
            if deps.workbench.stagedSessionIds.contains(session.id) {
                Button("Minimize") { deps.dismissSession(session.id) }
            }
            removeButton(session)
        }
    }

    @ViewBuilder
    private func removeButton(_ session: RuntimeSession) -> some View {
        if session.isOwner {
            Button("Remove from room") { remove(session) }
        }
    }

    private func remove(_ session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await deps.removeFromRoom(session)
            } catch { state.failure = error.localizedDescription }
        }
    }

    private var addMenu: some View {
        Button { adding.toggle() } label: {
            Image(systemName: "plus").font(.system(size: 11.5, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
                .frame(width: 28, height: 28).contentShape(Circle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.9))
        .help("Add a terminal").accessibilityLabel(Text("Add terminal")).accessibilityIdentifier("room.terminal.add")
        .kodosiPopover(isPresented: $adding, arrowEdge: .bottom, alignment: .leading) {
            VStack(alignment: .leading, spacing: 2) {
                PanelMenuRow(Text("New terminal"), icon: .symbol("plus"), identifier: "room.terminal.add.new") {
                    deps.openLauncher(roomId: roomId)
                }
                if !mine.isEmpty {
                    PanelMenuDivider()
                    PanelMenuHeader(title: "Share one of mine")
                    ForEach(mine) { session in
                        PanelMenuRow(Text(session.name), icon: .agent(AgentKind(program: session.program)),
                                     identifier: "room.terminal.add.\(AccessibilityIdentifier.token(session.id))")
                        {
                            share(session)
                        }
                    }
                }
            }
            .padding(6).frame(width: 220)
        }
    }

    private func open(_ session: RuntimeSession) {
        withAnimation(theme.motion.snappy) {
            state.selectedTerminal = session.id
            if session.canvas == nil {
                state.showsCanvas = false
            }
        }
        deps.activateSession(session.id, inRoom: roomId)
    }

    private func share(_ session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await deps.mutateSession("session.attachMission", session: session, fields: ["missionId": .string(roomId)])
                state.shownSection = 0; deps.activateSession(session.id, inRoom: roomId)
            } catch { state.failure = error.localizedDescription }
        }
    }
}

/// A room tab: the terminal's mark, its name and its status.
private struct TabLabel: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let selected: Bool

    var body: some View {
        HStack(spacing: 7) {
            AgentMark(session: session, size: 18, activity: session.mark(rested: selected ? .awake : .asleep))
            Text(session.name).appTextStyle(.footnote).fontWeight(.medium).lineLimit(1)
                .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
            if !session.isOwner, let owner = session.ownerName {
                PersonAvatar(name: owner, key: session.ownerUserId, size: 15)
            }
            if let sign = deps.sign(of: session) {
                StatusSign(form: sign, size: 10).transition(AnyTransition.pop)
            } else if session.isTroubled {
                Circle().fill(theme.colors.caution).frame(width: 5, height: 5)
            }
        }
        .padding(.leading, 6).padding(.trailing, 11).frame(height: 28)
    }
}

/// The chosen tab: a pill of the terminal's header bar, which flows down into the terminal.
private struct TabPill: View {
    @Environment(\.theme) private var theme

    var body: some View {
        Capsule().fill(theme.colors.band)
    }
}

/// The chosen tab is in the terminal's colors.
private struct TabScope: ViewModifier {
    let terminal: Bool

    func body(content: Content) -> some View {
        if terminal {
            content.terminalScope()
        } else {
            content
        }
    }
}
