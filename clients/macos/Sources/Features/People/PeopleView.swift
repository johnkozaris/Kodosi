import KodosiBridge
import KodosiDesign
import SwiftUI

struct PeopleView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var pendingRemoval: FriendEntry?
    @State private var blocking: BlockTarget?
    @State private var copied = false
    @FocusState private var adding: Bool

    private var facts: String {
        let waiting = deps.people.incoming.count
        let friends = Counted.friends(deps.people.friends.count)
        return waiting > 0 ? friends + " · " + String(localized: "\(waiting) waiting") : friends
    }

    var body: some View {
        if !deps.account.isReady {
            SignInView(title: "Add the people you work with", message: "Friends can open the terminals you share, and you can open theirs.")
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    PageHeader(title: "Friends", facts: facts) {
                        Button { deps.copyInvite() } label: {
                            Label(copied ? String(localized: "Invite copied") : String(localized: "Copy my invite"),
                                  systemImage: copied ? "checkmark" : "link")
                                .contentTransition(.symbolEffect(.replace))
                        }
                        .buttonStyle(.kodosi(copied ? .tinted : .secondary))
                        .accessibilityIdentifier("people.copyInvite")
                    }.arrive()
                    addCapsule.arrive(delay: 0.04)
                    if !deps.people.incoming.isEmpty || !deps.people.outgoing.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            ForEach(deps.people.incoming) { request in
                                FriendRequestCard(request: request, identifier: "people.request.\(request.handle)") {
                                    deps.people.decline(request)
                                } accept: {
                                    deps.workbench.trusting = .accept(request)
                                } block: {
                                    blocking = BlockTarget(handle: request.handle, name: request.personName)
                                }
                                .transition(AnyTransition.rise)
                            }
                            ForEach(deps.people.outgoing) { request in
                                OutgoingRequestCard(request: request, identifier: "people.request.cancel.\(request.handle)") {
                                    deps.people.cancel(request)
                                }
                                .transition(AnyTransition.rise)
                            }
                        }
                    }
                    if deps.people.friends.isEmpty {
                        EmptyState(title: "No friends yet", message: "Send your invite, or paste theirs above.") {
                            TogetherArt()
                        } actions: { EmptyView() }
                            .frame(height: 260)
                    } else {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 260, maximum: 360), spacing: 14, alignment: .top)], alignment: .leading, spacing: 14) {
                            ForEach(Array(deps.people.friends.enumerated()), id: \.element.id) { index, friend in
                                FriendItem(friend: friend) { pendingRemoval = friend } block: {
                                    blocking = BlockTarget(handle: friend.handle, name: friend.personName)
                                }
                                .arrive(delay: 0.04 * Double(min(index, 8) + 2))
                            }
                        }
                    }
                    if !deps.people.blocked.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Blocked").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
                            ForEach(deps.people.blocked) { person in
                                BlockedCard(person: person, identifier: "people.unblock.\(person.handle)") { deps.people.unblock(person) }
                                    .transition(AnyTransition.rise)
                            }
                        }
                    }
                }
                .blockConfirmation(isPresented: Binding(get: { blocking != nil }, set: {
                    if !$0 {
                        blocking = nil
                    }
                }), name: blocking?.name ?? "") {
                    if let target = blocking {
                        deps.people.block(handle: target.handle)
                    }
                    blocking = nil
                }
                .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 36)
                .frame(maxWidth: 1100, alignment: .leading).frame(maxWidth: .infinity)
            }
            .animation(theme.motion.spring, value: deps.people.friends.map(\.id))
            .animation(theme.motion.spring, value: deps.people.incoming.map(\.id))
            .animation(theme.motion.spring, value: deps.people.outgoing.map(\.id))
            .animation(theme.motion.spring, value: deps.people.blocked.map(\.id))
            .animation(theme.motion.snappy, value: copied)
            .confirmationDialog(
                "Remove \(pendingRemoval?.personName ?? "")?",
                isPresented: Binding(get: { pendingRemoval != nil }, set: {
                    if !$0 {
                        pendingRemoval = nil
                    }
                }), titleVisibility: .visible,
            ) {
                if let friend = pendingRemoval {
                    Button("Remove", role: .destructive) {
                        deps.people.remove(friend)
                        pendingRemoval = nil
                    }
                }
            } message: { Text("You stop sharing terminals with each other, and each of you leaves the rooms that the other owns. Kodosi does not undo the commands that they already ran.") }
            .onChange(of: deps.inviteCopies) { _, _ in copied = true }
            .task(id: copied) {
                guard copied else { return }
                try? await Task.sleep(for: .seconds(2))
                copied = false
            }
        }
    }

    private var addCapsule: some View {
        @Bindable var workbench = deps.workbench
        let trimmed = workbench.friendDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        return HStack(spacing: 10) {
            Image(systemName: "person.badge.plus").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(adding ? theme.colors.accentStrong : theme.colors.inkFaint)
            TextField("Paste a friend's invite, or type a username", text: $workbench.friendDraft)
                .textFieldStyle(.plain).appTextStyle(.callout).focused($adding)
                .onSubmit { add(trimmed) }
                .accessibilityIdentifier("people.username")
            Button("Add friend") { add(trimmed) }
                .buttonStyle(.kodosi(.primary)).disabled(trimmed.isEmpty)
                .accessibilityIdentifier("people.add")
        }
        .padding(.leading, 18).padding(.trailing, 7).frame(height: 48).frame(maxWidth: 620)
        .background {
            RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: adding ? .lifted : .resting)
        }
        .overlay { Capsule().strokeBorder(theme.colors.accent.opacity(adding ? 0.5 : 0), lineWidth: 1.5) }
        .animation(theme.motion.hover, value: adding)
    }

    private func add(_ name: String) {
        guard !name.isEmpty else { return }
        deps.workbench.trusting = .add(name)
    }
}

private struct BlockTarget: Equatable {
    let handle: String
    let name: String
}

private struct FriendItem: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var verifying = false
    @State private var invite = ""
    let friend: FriendEntry
    let remove: () -> Void
    let block: () -> Void

    var body: some View {
        let name = friend.personName
        FriendCard(friend: friend, shared: deps.listedSessions.filter { $0.ownerUserId == friend.userId }, openTerminal: open,
                   trust: trust, verify: toggleVerify)
        {
            Menu {
                if !friend.isVerified {
                    Button("Verify with their invite…") { verifying = true }
                }
                Button("Remove…", role: .destructive, action: remove)
                Button("Block…", role: .destructive, action: block)
            } label: {
                Image(systemName: "ellipsis").font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                    .frame(width: 26, height: 26).contentShape(Circle())
            }
            .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            .accessibilityLabel(Text("Options for \(name)"))
        } footer: {
            if verifying {
                HStack(spacing: 6) {
                    TextField("Paste the invite \(name) sent you", text: $invite)
                        .accessibilityIdentifier("people.verifyInvite.\(AccessibilityIdentifier.token(friend.id))")
                    Button("Verify") {
                        deps.people.verify(friend, invite: invite)
                        verifying = false; invite = ""
                    }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .disabled(invite.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                .transition(AnyTransition.unfold)
            }
        }
        .animation(theme.motion.spring, value: verifying)
    }

    private func open(_ session: RuntimeSession) {
        deps.activateSession(session.id)
    }

    private func trust() {
        deps.people.trust(friend)
    }

    private func toggleVerify() {
        verifying.toggle()
    }
}
