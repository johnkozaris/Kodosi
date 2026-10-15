import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var query = ""
    @State private var showsNewRoom = false
    @State private var opened: RoomRoute?
    @State private var confirming: MissionEntry?
    @State private var created: String?

    private var rooms: RoomStore {
        model.rooms
    }

    private var shown: [MissionEntry] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else { return rooms.missions }
        return rooms.missions.filter { $0.name.localizedStandardContains(needle) }
    }

    var body: some View {
        let shown = shown
        List {
            invitationsSection
            roomsSection(shown)
            placeholder
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background { GroundBackdrop() }
        .overlay {
            if !query.isEmpty, shown.isEmpty {
                ContentUnavailableView.search(text: query)
            }
        }
        .navigationTitle("Rooms")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { showsNewRoom = true } label: { Label("New room", systemImage: "plus") }
                    .accessibilityIdentifier("rooms.new")
            }
        }
        .searchable(text: $query, prompt: Text("Find a room"))
        .refreshable { await rooms.refreshList() }
        .task(id: rooms.missions.map(\.id)) { await rooms.prefetchDetails() }
        .animation(theme.motion.spring, value: rooms.missions.map(\.id))
        .animation(theme.motion.spring, value: rooms.invitations.map(\.id))
        .sheet(isPresented: $showsNewRoom, onDismiss: openCreated) {
            RoomCreateSheet { created = $0 }
        }
        .confirmationDialog(
            confirmationTitle, isPresented: confirmationShown, titleVisibility: .visible, presenting: confirming
        ) { mission in
            if rooms.isOwner(of: mission.id) {
                Button("Delete room", role: .destructive) { rooms.run(announcedByRuntime: true) { try await rooms.delete(mission.id) } }
                    .accessibilityIdentifier("rooms.delete.confirm")
            } else {
                Button("Leave room", role: .destructive) { rooms.run(announcedByRuntime: true) { try await rooms.leave(mission.id) } }
                    .accessibilityIdentifier("rooms.leave.confirm")
            }
        } message: { mission in
            if rooms.isOwner(of: mission.id) {
                Text("Its conversation and sharing end. Terminals keep running.")
            } else {
                Text("You lose its terminals and conversation until someone invites you again.")
            }
        }
        .navigationDestination(for: RoomRoute.self) { RoomScreen(roomId: $0.roomId) }
        .navigationDestination(for: TerminalRoute.self) { TerminalScreen(sessionId: $0.sessionId) }
        .navigationDestination(item: $opened) { RoomScreen(roomId: $0.roomId) }
    }

    @ViewBuilder
    private var invitationsSection: some View {
        if query.isEmpty, !rooms.invitations.isEmpty {
            Section("Invitations") {
                ForEach(rooms.invitations) { invitation in
                    RoomInvitationRow(invitation: invitation) { opened = RoomRoute(roomId: $0) }
                        .listRowBackground(theme.colors.raised)
                }
            }
        }
    }

    @ViewBuilder
    private func roomsSection(_ shown: [MissionEntry]) -> some View {
        if !shown.isEmpty {
            Section {
                ForEach(shown) { mission in
                    roomLink(mission)
                }
            } header: {
                if query.isEmpty, !rooms.invitations.isEmpty {
                    Text("Rooms")
                }
            } footer: {
                if rooms.listTruncated {
                    Text("Some rooms are not shown.")
                }
            }
        }
    }

    private func roomLink(_ mission: MissionEntry) -> some View {
        let owner = rooms.isOwner(of: mission.id)
        return NavigationLink(value: RoomRoute(roomId: mission.id)) {
            RoomRow(mission: mission)
        }
        .listRowBackground(theme.colors.raised)
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button(role: .destructive) { confirming = mission } label: {
                if owner {
                    Label("Delete", systemImage: "trash")
                } else {
                    Label("Leave", systemImage: "rectangle.portrait.and.arrow.right")
                }
            }
        }
        .contextMenu {
            if rooms.unread[mission.id] != nil {
                Button { rooms.markRead(mission.id) } label: { Label("Mark as read", systemImage: "checkmark.message") }
            }
            Button(role: .destructive) { confirming = mission } label: {
                if owner {
                    Label("Delete room…", systemImage: "trash")
                } else {
                    Label("Leave room…", systemImage: "rectangle.portrait.and.arrow.right")
                }
            }
        }
        .accessibilityIdentifier("rooms.row.\(AccessibilityIdentifier.token(mission.id))")
    }

    @ViewBuilder
    private var placeholder: some View {
        if !rooms.listed {
            RoomListSkeleton()
                .listRowBackground(Color.clear)
        } else if query.isEmpty, rooms.missions.isEmpty, rooms.invitations.isEmpty {
            EmptyState(title: "Make your first room", message: "A room keeps people, agents and terminals in one place.") {
                RoomSigil(key: "kodosi", size: 64)
            } actions: {
                Button { showsNewRoom = true } label: { Label("New room", systemImage: "plus") }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("rooms.empty.new")
            }
            .frame(minHeight: 420)
            .listRowBackground(Color.clear)
        }
    }

    private func openCreated() {
        guard let created else { return }
        self.created = nil
        opened = RoomRoute(roomId: created)
    }

    private var confirmationShown: Binding<Bool> {
        Binding(get: { confirming != nil }, set: {
            if !$0 {
                confirming = nil
            }
        })
    }

    private var confirmationTitle: String {
        guard let confirming else { return "" }
        return rooms.isOwner(of: confirming.id)
            ? String(localized: "Delete \(confirming.name)?")
            : String(localized: "Leave \(confirming.name)?")
    }
}

private struct RoomListSkeleton: View {
    var body: some View {
        VStack(spacing: 22) {
            ForEach(0 ..< 3, id: \.self) { index in
                HStack(alignment: .top, spacing: 12) {
                    SkeletonBlock(width: 46, height: 46, radius: 14)
                    VStack(alignment: .leading, spacing: 9) {
                        SkeletonBlock(width: 120, height: 12)
                        SkeletonBlock(height: 10)
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 150 : 210, height: 10)
                    }
                }
            }
        }
        .padding(.vertical, 8)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Loading"))
    }
}
