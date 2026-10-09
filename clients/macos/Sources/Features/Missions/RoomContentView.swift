import SwiftUI

struct RoomContentView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @Bindable var state: RoomViewState

    var body: some View {
        Group {
            if state.canvas == 0 {
                RoomTerminals(roomId: roomId, state: state)
            } else if deps.rooms[roomId] == nil {
                if let failure = state.failure {
                    RoomRecovery(message: failure) { Task { await deps.readRoom(roomId) } }
                } else {
                    RoomSkeleton()
                }
            } else if state.canvas == 1 {
                RoomTasksView(roomId: roomId, state: state)
            } else {
                RoomRepositoriesView(roomId: roomId, state: state)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .id(state.canvas)
        .transition(.opacity.combined(with: .offset(y: theme.motion.reduced ? 0 : 6)))
    }
}

private struct RoomTerminals: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Namespace private var tab
    let roomId: String
    @Bindable var state: RoomViewState

    private var sessions: [RuntimeSession] {
        deps.sessions.filter { $0.missionId == roomId }
    }

    private var mine: [RuntimeSession] {
        deps.sessions.filter { $0.isOwner && $0.kind == .local && $0.missionId != roomId }
    }

    var body: some View {
        let sessions = sessions
        let current = sessions.first { $0.id == state.selectedTerminal && deps.workbench.stagedSessionIds.contains($0.id) }
        VStack(spacing: 0) {
            if !sessions.isEmpty {
                HStack(spacing: 4) {
                    ViewThatFits(in: .horizontal) {
                        tabs(sessions, current: current)
                        ScrollView(.horizontal) { tabs(sessions, current: current) }.scrollIndicators(.never)
                    }
                    .wellCapsule()
                    .fixedSize(horizontal: false, vertical: true)
                    addMenu
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 8).padding(.bottom, 6)
                .animation(theme.motion.spring, value: sessions.map(\.id))
            }
            if let current {
                SessionTileView(session: current, isFocused: false, roomEmbedded: true)
                    .environment(\.terminalStageVisible, deps.workbench.section == .missions)
                    .environment(\.terminalInteractionEnabled, interactive)
                    .id(current.id)
                    .terminalScope()
                    .padding([.horizontal, .bottom], 6)
            } else if sessions.isEmpty {
                EmptyState(title: "No terminals here yet", message: "Add one. Everyone in the room gets full control.") {
                    NewTerminalGhost()
                } actions: {
                    Button { deps.newRoomTerminal(roomId) } label: { Label("New terminal", systemImage: "plus") }
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
                                RoomTerminalCard(session: session) { open(session) }
                            }
                        }
                    }
                    .padding(24).frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
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
        }
        .padding(3)
    }

    private func tabChip(_ session: RuntimeSession, selected: Bool) -> some View {
        Button { open(session) } label: {
            HStack(spacing: 7) {
                AgentMark(kind: session.agent, size: 18, activity: session.mark(rested: selected ? .awake : .asleep))
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
            .background {
                if selected {
                    RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                        .matchedGeometryEffect(id: "tab", in: tab)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help("\(session.hostLabel) · \(session.folderName ?? "")")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("room.terminal.\(AccessibilityIdentifier.token(session.id))")
        .contextMenu {
            Button("Details") { deps.workbench.detailsSessionId = session.id }
            if deps.workbench.stagedSessionIds.contains(session.id) {
                Button("Minimize") { deps.dismissSession(session.id) }
            }
        }
    }

    private var addMenu: some View {
        Menu {
            Button("New terminal") { deps.newRoomTerminal(roomId) }
            if !mine.isEmpty {
                Section("Share one of mine") {
                    ForEach(mine) { session in Button(session.name) { share(session) } }
                }
            }
        } label: {
            Image(systemName: "plus").font(.system(size: 12, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
                .frame(width: 30, height: 30).contentShape(Circle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
        .help("Add a terminal").accessibilityLabel(Text("Add terminal")).accessibilityIdentifier("room.terminal.add")
    }

    private func open(_ session: RuntimeSession) {
        withAnimation(theme.motion.snappy) { state.selectedTerminal = session.id }
        deps.activateSession(session.id, inRoom: roomId)
    }

    private func share(_ session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await deps.mutateSession("session.attachMission", session: session, fields: ["missionId": .string(roomId)])
                state.canvas = 0; deps.activateSession(session.id, inRoom: roomId)
            } catch { state.failure = error.localizedDescription }
        }
    }
}

private struct RoomTerminalCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let session: RuntimeSession
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                AgentMark(kind: session.agent, size: 34, activity: session.mark(rested: session.canOpen ? .awake : .asleep))
                VStack(alignment: .leading, spacing: 3) {
                    Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    HStack(spacing: 4) {
                        DeviceGlyph(label: session.hostLabel).font(.system(size: 9, weight: .medium))
                        SessionActivityText(session: session, fallback: session.hostLabel)
                    }
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer(minLength: 0)
                if !session.isOwner, let owner = session.ownerName {
                    PersonAvatar(name: owner, key: session.ownerUserId, size: 20)
                }
            }
            .padding(.horizontal, 14).frame(height: 68)
            .raised(Radius.lg, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
            .offset(y: hovered ? -1 : 0)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .onHover { hovered = $0 }
        .animation(theme.motion.spring, value: hovered)
        .accessibilityIdentifier("room.terminal.card.\(AccessibilityIdentifier.token(session.id))")
    }
}

struct NewTerminalGhost: View {
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "chevron.right").font(.system(size: 16, weight: .heavy)).foregroundStyle(theme.colors.inkFaint)
            CursorBlock(width: 11, height: 20)
            Spacer()
        }
        .padding(16).frame(width: 190, height: 96, alignment: .topLeading)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                .strokeBorder(theme.colors.inkFaint.opacity(0.5), style: StrokeStyle(lineWidth: 1.2, dash: [5, 5]))
        }
        .accessibilityHidden(true)
    }
}
