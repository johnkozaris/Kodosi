import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openRoute) private var openRoute
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
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(theme.colors.surface)
        .readableWidth()
        .overlay {
            if !query.isEmpty, shown.isEmpty {
                NothingMatches()
            }
        }
        .navigationTitle("Projects")
        .pageFacts(facts)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                IconButton(title: "New project", symbol: "plus", identifier: "rooms.new", size: 32) { showsNewRoom = true }
            }
            .sharedBackgroundVisibility(.hidden)
        }
        .searchable(text: $query, prompt: Text("Find a project"))
        .refreshable { await rooms.refreshList() }
        .task(id: rooms.missions.map(\.id)) { await rooms.prefetchDetails() }
        .animation(theme.motion.spring, value: rooms.missions.map(\.id))
        .animation(theme.motion.spring, value: rooms.invitations.map(\.id))
        .sheet(isPresented: $showsNewRoom, onDismiss: openCreated) {
            RoomCreateSheet { created = $0 }
        }
        .confirmationDialog(
            confirmationTitle, isPresented: confirmationShown, titleVisibility: .visible, presenting: confirming,
        ) { mission in
            if rooms.isOwner(of: mission.id) {
                Button("Delete project", role: .destructive) { rooms.run { try await rooms.delete(mission.id) } }
                    .accessibilityIdentifier("rooms.delete.confirm")
            } else {
                Button("Leave project", role: .destructive) { rooms.run { try await rooms.leave(mission.id) } }
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
            ForEach(rooms.invitations) { invitation in
                RoomInvitationRow(invitation: invitation) { opened = RoomRoute(roomId: $0) }
                    .cardRow(top: 6, bottom: 6)
            }
        }
    }

    @ViewBuilder
    private func roomsSection(_ shown: [MissionEntry]) -> some View {
        ForEach(shown) { mission in
            roomCard(mission)
        }
        if query.isEmpty, rooms.listed, !rooms.missions.isEmpty {
            Button { showsNewRoom = true } label: {
                DashedSlot(radius: Radius.xl) {
                    HStack(spacing: 10) {
                        Image(systemName: "plus").font(.system(size: 16, weight: .semibold))
                        Text("New project").appTextStyle(.subhead)
                    }
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity, minHeight: 64)
                }
            }
            .buttonStyle(PressScaleStyle(scale: 0.98))
            .accessibilityIdentifier("rooms.slot.new")
            .cardRow(top: 6, bottom: 6)
        }
        if rooms.listTruncated {
            Text("Some projects are not shown.")
                .appTextStyle(.caption).fontWeight(.regular)
                .foregroundStyle(theme.colors.inkFaint)
                .cardRow()
        }
    }

    private func roomCard(_ mission: MissionEntry) -> some View {
        let owner = rooms.isOwner(of: mission.id)
        return RoomListCard(mission: mission) { openRoute(RoomRoute(roomId: mission.id)) }
            .cardRow(top: 6, bottom: 6)
            .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                Button(role: .destructive) { confirming = mission } label: {
                    if owner {
                        Label("Delete", systemImage: "trash")
                    } else {
                        Label("Leave", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                }
                .tint(theme.colors.danger)
            }
            .contextMenu {
                if mission.unread {
                    Button { rooms.markRead(mission.id) } label: { Label("Mark as read", systemImage: "checkmark.message") }
                }
                Button(role: .destructive) { confirming = mission } label: {
                    if owner {
                        Label("Delete project…", systemImage: "trash")
                    } else {
                        Label("Leave project…", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                }
            }
            .accessibilityIdentifier("rooms.row.\(AccessibilityIdentifier.token(mission.id))")
    }

    @ViewBuilder
    private var placeholder: some View {
        if !rooms.listed {
            RoomListSkeleton()
                .cardRow()
        } else if query.isEmpty, rooms.missions.isEmpty, rooms.invitations.isEmpty {
            EmptyState(title: "Make your first project", message: "A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.") {
                RoomSigil(key: "kodosi", size: 64)
            } actions: {
                Button { showsNewRoom = true } label: { Label("New project", systemImage: "plus") }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("rooms.empty.new")
            }
            .frame(minHeight: 420)
            .cardRow()
        }
    }

    private var facts: String {
        guard !rooms.missions.isEmpty else { return "" }
        let shared = model.terminals.sessions.count { $0.missionId != nil && $0.status != .closing }
        return "\(Counted.rooms(rooms.missions.count)) · " + String(localized: "\(shared) shared terminals")
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
        VStack(spacing: 12) {
            ForEach(0 ..< 3, id: \.self) { index in
                VStack(alignment: .leading, spacing: 10) {
                    SkeletonBlock(width: 44, height: 44, radius: 13)
                    SkeletonBlock(width: 120, height: 12)
                    SkeletonBlock(width: index.isMultiple(of: 2) ? 150 : 210, height: 10)
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .raised(Radius.xl)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Loading"))
    }
}
