import SwiftUI

struct RoomContentView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let roomId: String
    @Bindable var state: RoomViewState
    @Namespace private var selection

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) {
                dockItem(0, "Terminals", "terminal", deps.sessions.filter { $0.missionId == roomId }.count)
                dockItem(1, "Tasks", "checklist", deps.rooms[roomId]?.tasks.filter { !$0.closed }.count ?? 0)
                dockItem(2, "Repositories", "point.3.connected.trianglepath.dotted", deps.rooms[roomId]?.repositories.count ?? 0)
                Spacer(minLength: 4)
                if state.canvas == 0 {
                    Button { deps.newRoomTerminal(roomId) } label: { Image(systemName: "plus") }
                        .help("New terminal").accessibilityLabel(Text("New terminal")).accessibilityIdentifier("room.terminal.new")
                }
            }.buttonStyle(.plain).padding(10).background(theme.colors.surfacePanel).seamBorder(.bottom)
            Group {
                if state.canvas == 0 {
                    terminals
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
            .transition(.opacity.combined(with: .offset(y: reduceMotion ? 0 : 5)))
        }.background(theme.colors.surfaceStage)
    }

    private func dockItem(_ index: Int, _ title: LocalizedStringKey, _ symbol: String, _ count: Int) -> some View {
        Button {
            withAnimation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.85)) { state.canvas = index }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: symbol)
                Text(title)
                if count > 0 {
                    Text(count, format: .number).font(.system(size: 11, weight: .semibold)).contentTransition(.numericText())
                }
            }.font(.system(size: 12, weight: .medium)).padding(.horizontal, 10).padding(.vertical, 8)
                .foregroundStyle(state.canvas == index ? theme.colors.foreground : theme.colors.mutedForeground)
                .background {
                    if state.canvas == index {
                        RoundedRectangle(cornerRadius: 9).fill(theme.colors.secondary).matchedGeometryEffect(id: "canvas", in: selection)
                    }
                }
        }.accessibilityAddTraits(state.canvas == index ? .isSelected : []).accessibilityIdentifier("room.canvas.\(index)")
    }

    private var terminals: some View {
        let sessions = deps.sessions.filter { $0.missionId == roomId }
        let current = sessions.first { $0.id == state.selectedTerminal && deps.workbench.stagedSessionIds.contains($0.id) }
        return VStack(spacing: 0) {
            if current == nil, !sessions.isEmpty {
                ScrollView(.horizontal) {
                    HStack(spacing: 6) {
                        ForEach(sessions) { session in
                            Button { state.selectedTerminal = session.id; deps.activateSession(session.id, inRoom: roomId) } label: {
                                HStack(spacing: 7) {
                                    SessionProgramIcon(program: session.program)
                                    Text(session.name).lineLimit(1)
                                    Circle().fill(session.canControl ? theme.colors.tertiary : theme.colors.mutedForeground).frame(
                                        width: 5,
                                        height: 5
                                    )
                                }.font(.system(size: 12)).padding(.horizontal, 10).padding(.vertical, 8)
                                    .background(current?.id == session.id ? theme.colors.secondary : .clear, in: RoundedRectangle(cornerRadius: 8))
                            }.buttonStyle(.plain).help(session.hostLabel)
                                .accessibilityIdentifier("room.terminal.\(AccessibilityIdentifier.token(session.id))")
                        }
                    }.padding(8)
                }.scrollIndicators(.hidden).seamBorder(.bottom)
            }
            if let current {
                SessionTileView(session: current, isFocused: false, roomEmbedded: true)
                    .environment(\.terminalStageVisible, deps.workbench.section == .missions)
                    .environment(
                        \.terminalInteractionEnabled,
                        deps.workbench.section == .missions && deps.workbench.detailsSessionId == nil && deps.workbench.sharingSessionId == nil
                    )
                    .id(current.id)
            } else {
                VStack(spacing: 20) {
                    Image(systemName: "terminal").font(.system(size: 52, weight: .ultraLight)).foregroundStyle(theme.colors.primary)
                    if sessions.isEmpty {
                        Button { deps.newRoomTerminal(roomId) } label: { Label("New terminal", systemImage: "plus") }
                            .buttonStyle(SolidPrimaryButtonStyle())
                    } else {
                        Text("Select a terminal").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }
}
