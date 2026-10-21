import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomInfoSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let roomId: String
    let left: () -> Void
    @State private var invited: Set<String> = []
    @State private var renaming = false
    @State private var newName = ""
    @State private var confirmingEnd = false
    @State private var removing: MissionMemberEntry?
    @State private var busy = false
    @State private var failure: String?
    @State private var done = 0

    private var rooms: RoomStore {
        model.rooms
    }

    private var owner: Bool {
        rooms.isOwner(of: roomId)
    }

    private var name: String {
        rooms.mission(roomId)?.name ?? ""
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(spacing: 12) {
                        RoomSigil(key: roomId, size: 64)
                        Text(name)
                            .appTextStyle(.title)
                            .foregroundStyle(theme.colors.ink)
                            .multilineTextAlignment(.center)
                        Label("Encrypted end to end", systemImage: "lock.fill")
                            .appTextStyle(.caption)
                            .foregroundStyle(theme.colors.inkMuted)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
                    .listRowBackground(Color.clear)
                }
                RoomMembersSection(roomId: roomId, owner: owner) { removing = $0 }
                if owner {
                    inviteSection
                }
                RoomRepositoriesSection(roomId: roomId)
                if let failure {
                    Section {
                        ErrorNote(message: failure)
                            .listRowInsets(EdgeInsets())
                            .listRowBackground(Color.clear)
                    }
                }
                Section {
                    if owner {
                        Button {
                            newName = name
                            renaming = true
                        } label: {
                            Label("Rename", systemImage: "pencil")
                        }
                        .accessibilityIdentifier("rooms.rename")
                    }
                    Button(role: .destructive) { confirmingEnd = true } label: {
                        if owner {
                            Label("Delete room…", systemImage: "trash")
                        } else {
                            Label("Leave room…", systemImage: "rectangle.portrait.and.arrow.right")
                        }
                    }
                    .accessibilityIdentifier(owner ? "rooms.delete" : "rooms.leave")
                }
                .disabled(busy)
                .listRowBackground(theme.colors.raised)
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(theme.colors.surface)
            .navigationTitle("Room")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .accessibilityIdentifier("rooms.info.done")
                }
            }
            .alert("Rename room", isPresented: $renaming) {
                TextField("Room name", text: $newName)
                    .accessibilityIdentifier("rooms.rename.name")
                Button("Cancel", role: .cancel) {}
                Button("Rename") { perform { try await rooms.rename(roomId, to: newName) } }
                    .disabled(!ProductInput.validName(newName))
            }
            .confirmationDialog(
                owner ? String(localized: "Delete \(name)?") : String(localized: "Leave \(name)?"),
                isPresented: $confirmingEnd, titleVisibility: .visible,
            ) {
                Button(owner ? "Delete room" : "Leave room", role: .destructive, action: end)
            } message: {
                if owner {
                    Text("Its conversation and sharing end. Terminals keep running.")
                } else {
                    Text("You lose its terminals and conversation until someone invites you again.")
                }
            }
            .confirmationDialog(
                String(localized: "Remove \(removing?.personName ?? "")?"),
                isPresented: Binding(get: { removing != nil }, set: {
                    if !$0 {
                        removing = nil
                    }
                }),
                titleVisibility: .visible, presenting: removing,
            ) { member in
                Button("Remove from room", role: .destructive) {
                    perform { try await rooms.remove(member, from: roomId) }
                }
            } message: { _ in
                Text("They lose its terminals and conversation until you invite them again.")
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
        .sensoryFeedback(.success, trigger: done)
        .animation(theme.motion.spring, value: failure)
    }

    @ViewBuilder
    private var inviteSection: some View {
        let members = Set((rooms.details[roomId]?.members ?? []).map(\.userId))
        let candidates = model.people.friends.filter { !members.contains($0.userId) }
        Section {
            if candidates.isEmpty {
                Text(model.people.friends.isEmpty ? "Add friends in the Friends tab to invite them." : "All your friends are here.")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
            }
            ForEach(candidates) { friend in
                inviteRow(friend)
            }
        } header: {
            Text("Invite friends")
        }
        .listRowBackground(theme.colors.raised)
    }

    private func inviteRow(_ friend: FriendEntry) -> some View {
        let name = friend.personName
        let sent = invited.contains(friend.userId)
        return HStack(spacing: 12) {
            PersonAvatar(name: name, key: friend.userId, size: 34)
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                if friend.identityChanged {
                    Text("Their identity changed. Trust them in Friends first.")
                        .appTextStyle(.caption)
                        .foregroundStyle(theme.colors.caution)
                } else {
                    Text(verbatim: "@\(friend.handle)")
                        .appTextStyle(.caption)
                        .foregroundStyle(theme.colors.inkFaint)
                }
            }
            Spacer(minLength: 8)
            Button {
                invite(friend)
            } label: {
                Label(sent ? "Invited" : "Invite", systemImage: sent ? "checkmark" : "paperplane")
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.kodosi(sent ? .ghost : .tinted, size: .small))
            .disabled(sent || friend.identityChanged)
            .accessibilityIdentifier("rooms.invite.\(AccessibilityIdentifier.token(friend.userId))")
        }
        .animation(theme.motion.snappy, value: sent)
    }

    private func invite(_ friend: FriendEntry) {
        invited.insert(friend.userId)
        failure = nil
        let rooms = rooms
        let roomId = roomId
        Task {
            do {
                try await rooms.invite(friend.userId, to: roomId)
                done += 1
            } catch is CancellationError {
                invited.remove(friend.userId)
            } catch {
                invited.remove(friend.userId)
                failure = error.localizedDescription
            }
        }
    }

    private func end() {
        let owner = owner
        perform {
            if owner {
                try await rooms.delete(roomId)
            } else {
                try await rooms.leave(roomId)
            }
            left()
        }
    }

    private func perform(_ work: @escaping () async throws -> Void) {
        guard !busy else { return }
        busy = true
        failure = nil
        Task {
            defer { busy = false }
            do {
                try await work()
                done += 1
            } catch is CancellationError {
                return
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}
