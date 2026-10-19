import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.dismiss) private var dismiss
    let roomId: String
    @State private var width: CGFloat = 0
    @State private var terminal: TerminalRoute?
    @State private var leaving = false

    private var rooms: RoomStore {
        model.rooms
    }

    private var wide: Bool {
        sizeClass == .regular && width >= 720
    }

    private var present: Bool {
        rooms.mission(roomId) != nil && !(rooms.departed.contains(roomId) && !leaving)
    }

    var body: some View {
        @Bindable var state = rooms.view(roomId)
        content(state)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background { GroundBackdrop() }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .navigationTitle(rooms.mission(roomId)?.name ?? "")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar(state) }
            .toolbarVisibility(wide ? .automatic : .hidden, for: .tabBar)
            .safeAreaBar(edge: .top) {
                if present, !wide {
                    dock(state, sections: [.conversation, .tasks, .terminals], selection: state.section) { state.section = $0 }
                }
            }
            .task(id: roomId) { await rooms.open(roomId) }
            .onAppear { rooms.visibleRoomId = roomId }
            .onDisappear {
                if rooms.visibleRoomId == roomId {
                    rooms.visibleRoomId = nil
                }
            }
            .sheet(isPresented: $state.showsInfo, onDismiss: finishLeaving) {
                RoomInfoSheet(roomId: roomId) {
                    leaving = true
                    state.showsInfo = false
                }
            }
            .sheet(item: $state.selectedText) { RoomTextSheet(text: $0.text) }
            .navigationDestination(item: $terminal) { TerminalScreen(sessionId: $0.sessionId) }
            .environment(\.openURL, OpenURLAction(handler: open))
            .sensoryFeedback(.selection, trigger: state.section) { _, _ in !wide }
            .sensoryFeedback(.selection, trigger: state.canvas) { _, _ in wide }
    }

    @ViewBuilder
    private func content(_ state: RoomViewState) -> some View {
        if leaving {
            Color.clear
        } else if rooms.departed.contains(roomId) {
            EmptyState(title: "You are no longer in this room", message: "It ended, or someone removed you.") {
                RoomSigil(key: roomId, size: 56).opacity(0.55)
            } actions: {
                Button("Back to rooms") { dismiss() }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("rooms.departed.back")
            }
        } else if rooms.mission(roomId) == nil {
            if let failure = rooms.detailFailures[roomId] {
                RoomRecovery(message: failure) { Task { await rooms.open(roomId) } }
            } else {
                RoomSkeleton()
            }
        } else if wide {
            HStack(alignment: .top, spacing: 12) {
                VStack(spacing: 0) {
                    dock(state, sections: [.terminals, .tasks], selection: state.canvas) { state.canvas = $0 }
                    section(state.canvas == .conversation ? .terminals : state.canvas)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                RoomConversation(roomId: roomId)
                    .frame(width: min(480, max(340, width * 0.4)))
                    .clipShape(RoundedRectangle(cornerRadius: Radius.xl, style: .continuous))
                    .raised(Radius.xl, fill: theme.colors.raised.mix(with: theme.colors.surface, by: 0.45))
                    .padding(.bottom, 12)
            }
            .padding(.trailing, 12)
        } else {
            section(state.section)
        }
    }

    private func section(_ section: RoomSection) -> some View {
        Group {
            switch section {
            case .conversation: RoomConversation(roomId: roomId)
            case .tasks: RoomTasks(roomId: roomId)
            case .terminals: RoomTerminals(roomId: roomId)
            }
        }
        .id(section)
        .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: theme.motion.reduced ? 0 : 6)), removal: .identity))
        .animation(theme.motion.snappy, value: section)
    }

    private func dock(
        _ state: RoomViewState, sections: [RoomSection], selection: RoomSection, select: @escaping (RoomSection) -> Void
    ) -> some View {
        let tasks = rooms.snapshots[roomId]?.tasks ?? []
        let open = tasks.filter { !$0.closed }.count
        let terminals = model.terminals.sessions.filter { $0.missionId == roomId }.count
        let items = sections.map { section in
            switch section {
            case .conversation:
                DockItem(id: section.rawValue, title: String(localized: "Conversation"), symbol: "bubble.left.and.bubble.right",
                         count: rooms.unread[roomId] ?? 0)
            case .tasks:
                DockItem(id: section.rawValue, title: String(localized: "Tasks"), symbol: "checklist", count: open,
                         progress: tasks.isEmpty ? nil : Double(tasks.count - open) / Double(tasks.count), fresh: state.freshTasks)
            case .terminals:
                DockItem(id: section.rawValue, title: String(localized: "Terminals"), symbol: "apple.terminal", count: terminals)
            }
        }
        return SectionDock(items: items, selection: selection.rawValue, identifier: "rooms.section") { id in
            if let section = RoomSection(rawValue: id) {
                select(section)
            }
        }
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity)
    }

    @ToolbarContentBuilder
    private func toolbar(_ state: RoomViewState) -> some ToolbarContent {
        if let mission = rooms.mission(roomId) {
            ToolbarItem(placement: .principal) {
                Button { state.showsInfo = true } label: { RoomHeader(mission: mission) }
                    .buttonStyle(PressScaleStyle(scale: 0.97))
                    .accessibilityHint(Text("Shows the people and settings of this room"))
                    .accessibilityIdentifier("rooms.header")
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { state.showsInfo = true } label: {
                    AvatarStack(people: members, size: 28, limit: 3, ring: theme.colors.surface)
                }
                .buttonStyle(PressScaleStyle())
                .accessibilityLabel(Text("Room members"))
                .accessibilityIdentifier("rooms.members")
            }
            .sharedBackgroundVisibility(.hidden)
        }
    }

    private var members: [AvatarStack.Person] {
        (rooms.details[roomId]?.members ?? []).map { member in
            AvatarStack.Person(id: member.userId, name: member.personName, isSelf: member.userId == rooms.userId)
        }
    }

    private func open(_ url: URL) -> OpenURLAction.Result {
        guard let id = RoomMentions.terminalId(in: url) else { return .systemAction }
        if model.terminals.session(id) != nil {
            terminal = TerminalRoute(sessionId: id)
        } else {
            model.notices.show(String(localized: "This terminal is not available."))
        }
        return .handled
    }

    private func finishLeaving() {
        if leaving {
            dismiss()
        }
    }
}

private struct RoomHeader: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let mission: MissionEntry

    var body: some View {
        let people = model.rooms.details[mission.id]?.members.count
        let terminals = model.terminals.sessions.filter { $0.missionId == mission.id }.count
        HStack(spacing: 9) {
            RoomSigil(key: mission.id, size: 28)
            VStack(alignment: .leading, spacing: 0) {
                Text(mission.name)
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                HStack(spacing: 4) {
                    Image(systemName: "lock.fill").font(.system(size: 8, weight: .semibold))
                    Text(verbatim: [people.map(Counted.people), Counted.terminals(terminals)].compactMap(\.self).joined(separator: " · "))
                        .contentTransition(.numericText())
                }
                .appTextStyle(.caption2)
                .foregroundStyle(theme.colors.inkMuted)
                .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}
