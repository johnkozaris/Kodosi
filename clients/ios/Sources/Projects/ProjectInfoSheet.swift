import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectInfoSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let projectId: String
    let left: () -> Void
    @State private var invited: Set<String> = []
    @State private var renaming = false
    @State private var confirmingEnd = false
    @State private var removing: ProjectMemberEntry?
    @State private var busy = false
    @State private var failure: String?
    @State private var done = 0

    private var projects: ProjectStore {
        model.projects
    }

    private var owner: Bool {
        projects.isOwner(of: projectId)
    }

    private var name: String {
        projects.project(projectId)?.name ?? ""
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    VStack(spacing: 12) {
                        ProjectSigil(key: projectId, size: 64)
                        Text(name)
                            .appTextStyle(.title)
                            .foregroundStyle(theme.colors.ink)
                            .multilineTextAlignment(.center)
                        Label("Encrypted end to end", systemImage: "lock.fill")
                            .appTextStyle(.caption)
                            .foregroundStyle(theme.colors.inkMuted)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 6)
                    ProjectMembersSection(projectId: projectId, owner: owner) { removing = $0 }
                    if owner {
                        inviteSection
                    }
                    ProjectRepositoriesSection(projectId: projectId)
                    if let failure {
                        ErrorNote(message: failure).transition(AnyTransition.rise)
                    }
                    ListGroup {
                        if owner {
                            Button { renaming = true } label: {
                                ListRow(String(localized: "Rename"), symbol: "pencil", tint: TileTint.graphite)
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .accessibilityIdentifier("projects.rename")
                        }
                        Button { confirmingEnd = true } label: {
                            ListRow(owner ? String(localized: "Delete project…") : String(localized: "Leave project…"),
                                    symbol: owner ? "trash.fill" : "rectangle.portrait.and.arrow.right", tint: TileTint.red,
                                    titleColor: theme.colors.danger)
                        }
                        .buttonStyle(ListRowButtonStyle())
                        .accessibilityIdentifier(owner ? "projects.delete" : "projects.leave")
                    }
                    .disabled(busy)
                }
                .padding(20)
                .frame(maxWidth: 640)
                .frame(maxWidth: .infinity)
            }
            .background(theme.colors.surface)
            .navigationTitle("Project")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Done", symbol: "xmark", identifier: "projects.info.done", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
            .sheet(isPresented: $renaming) {
                RenameSheet(title: "Rename project", field: "Project name", name: name, identifier: "projects.rename") { newName in
                    perform { try await projects.rename(projectId, to: newName) }
                }
            }
            .confirmationDialog(
                owner ? String(localized: "Delete \(name)?") : String(localized: "Leave \(name)?"),
                isPresented: $confirmingEnd, titleVisibility: .visible,
            ) {
                Button(owner ? "Delete project" : "Leave project", role: .destructive, action: end)
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
                Button("Remove from project", role: .destructive) {
                    perform { try await projects.remove(member, from: projectId) }
                }
            } message: { _ in
                Text("They lose its terminals and conversation until you invite them again.")
            }
        }
        .presentationDetents([.medium, .large])
        .kodosiSheet()
        .sensoryFeedback(.success, trigger: done)
        .animation(theme.motion.spring, value: failure)
    }

    @ViewBuilder
    private var inviteSection: some View {
        let candidates = projects.invitable(model.people.friends, to: projectId)
        ListGroup(title: "Invite friends") {
            if candidates.isEmpty {
                Text(model.people.friends.isEmpty ? "Add friends in Settings to invite them." : "All your friends are here.")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            ForEach(candidates) { friend in
                inviteRow(friend)
            }
        }
    }

    private func inviteRow(_ friend: FriendEntry) -> some View {
        let sent = invited.contains(friend.userId)
        return ListRow(title: friend.personName,
                       subtitle: friend.identityChanged ? String(localized: "Their identity changed. Trust them in Friends first.") : "@\(friend.handle)")
        {
            PersonAvatar(name: friend.personName, key: friend.userId, size: 28)
        } trailing: {
            Button {
                invite(friend)
            } label: {
                Label(sent ? "Invited" : "Invite", systemImage: sent ? "checkmark" : "paperplane")
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.kodosi(sent ? .ghost : .tinted, size: .small))
            .disabled(sent || friend.identityChanged)
            .accessibilityIdentifier("projects.invite.\(AccessibilityIdentifier.token(friend.userId))")
        }
        .animation(theme.motion.snappy, value: sent)
    }

    private func invite(_ friend: FriendEntry) {
        invited.insert(friend.userId)
        failure = nil
        let projects = projects
        let projectId = projectId
        Task {
            do {
                try await projects.invite(friend.userId, to: projectId)
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
                try await projects.delete(projectId)
            } else {
                try await projects.leave(projectId)
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
