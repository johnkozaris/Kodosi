import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct PeopleHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var adding = false
    @State private var trusting: FriendTrust?

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        List {
            if !people.loaded {
                loading
            } else if people.isEmpty, !people.sendingRequest {
                empty
                blocked
            } else {
                requests
                friends
                blocked
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(theme.colors.surface)
        .readableWidth()
        .navigationTitle("Friends")
        .pageFacts(people.facts)
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                InviteShareButton(identifier: "people.shareInvite") {
                    IconButtonFace(symbol: "square.and.arrow.up", size: 32)
                        .accessibilityLabel(Text("Share my invite"))
                }
                .buttonStyle(PressScaleStyle())
                IconButton(title: "Add a friend", symbol: "person.badge.plus", identifier: "people.add", size: 32) { adding = true }
            }
            .sharedBackgroundVisibility(.hidden)
        }
        .sheet(isPresented: $adding) { AddFriendSheet() }
        .friendTrust($trusting) { trust in
            if case let .accept(request) = trust {
                people.accept(request)
            }
        }
        .refreshable { await people.refresh() }
        .task {
            if people.ownInvite == nil {
                people.requestInvite()
            }
        }
        .animation(theme.motion.spring, value: people.friends.map(\.id))
        .animation(theme.motion.spring, value: people.incoming.map(\.id))
        .animation(theme.motion.spring, value: people.outgoing.map(\.id))
        .animation(theme.motion.spring, value: people.blocked.map(\.id))
        .animation(theme.motion.soft, value: people.loaded)
        .animation(theme.motion.snappy, value: people.sendingRequest)
        .sensoryFeedback(trigger: FriendTally(loaded: people.loaded, count: people.friends.count)) { old, new in
            old.loaded && new.count > old.count ? .success : nil
        }
        .sensoryFeedback(trigger: people.inFlight.count + (people.sendingRequest ? 1 : 0)) { old, new in
            new > old ? .impact(weight: .light) : nil
        }
    }

    private var loading: some View {
        ForEach(0 ..< 3, id: \.self) { index in
            HStack(spacing: 12) {
                SkeletonBlock(width: 46, height: 46, radius: 23)
                VStack(alignment: .leading, spacing: 8) {
                    SkeletonBlock(width: 140, height: 12)
                    SkeletonBlock(width: 90, height: 10)
                }
                Spacer(minLength: 0)
            }
            .padding(16)
            .raised(Radius.xl)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text("Loading friends"))
            .accessibilityHidden(index > 0)
            .cardRow(top: 6, bottom: 6)
        }
    }

    private var empty: some View {
        EmptyState(
            title: "No friends yet",
            message: """
            Send your invite to a friend, or add them with their invite or username. \
            Friends can open the terminals you share, and you can open theirs.
            """,
        ) {
            TogetherArt(size: 52)
        } actions: {
            VStack(spacing: 10) { emptyActions }
                .frame(maxWidth: 300)
        }
        .padding(.top, 24)
        .cardRow()
    }

    @ViewBuilder
    private var emptyActions: some View {
        Button { adding = true } label: {
            Label("Add a friend", systemImage: "person.badge.plus")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.kodosi(.primary, size: .large))
        .accessibilityIdentifier("people.empty.add")
        InviteShareButton(identifier: "people.empty.shareInvite") {
            Label("Share my invite", systemImage: "square.and.arrow.up")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.kodosi(.secondary, size: .large))
    }

    @ViewBuilder
    private var requests: some View {
        if !people.incoming.isEmpty || !people.outgoing.isEmpty || people.sendingRequest {
            ListHeader(title: "Requests", count: people.incoming.count)
                .cardRow(top: 14, bottom: 2)
            ForEach(people.incoming) { request in
                IncomingRequestRow(request: request) { trusting = .accept(request) }
                    .cardRow(top: 6, bottom: 6)
            }
            ForEach(people.outgoing) { request in
                OutgoingRequestRow(request: request)
                    .cardRow()
            }
            if people.sendingRequest {
                ProgressCaption(text: String(localized: "Sending your request"))
                    .padding(.vertical, 8)
                    .cardRow()
                    .accessibilityIdentifier("people.sending")
            }
        }
    }

    @ViewBuilder
    private var friends: some View {
        if !people.friends.isEmpty {
            ListHeader(title: "Friends", count: people.friends.count)
                .cardRow(top: 14, bottom: 2)
            ForEach(people.friends) { friend in
                FriendListItem(friend: friend, terminals: model.terminals.sessions.filter { $0.ownerUserId == friend.userId && !$0.hidden })
                    .cardRow(top: 6, bottom: 6)
            }
        }
    }

    @ViewBuilder
    private var blocked: some View {
        if !people.blocked.isEmpty {
            ListHeader(title: "Blocked", count: people.blocked.count)
                .cardRow(top: 14, bottom: 2)
            ForEach(people.blocked) { person in
                BlockedCard(person: person, busy: people.isBusy(person.handle), identifier: "people.unblock.\(person.handle)") {
                    people.unblock(person)
                }
                .cardRow()
            }
        }
    }
}

extension PeopleStore {
    var facts: String {
        guard loaded, !friends.isEmpty || !incoming.isEmpty else { return "" }
        let count = Counted.friends(friends.count)
        return incoming.isEmpty ? count : count + " · " + String(localized: "\(incoming.count) waiting")
    }
}

private struct FriendTally: Equatable {
    let loaded: Bool
    let count: Int
}

private struct FriendListItem: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openRoute) private var openRoute
    @State private var confirmingRemoval = false
    let friend: FriendEntry
    let terminals: [RuntimeSession]

    private var people: PeopleStore {
        model.people
    }

    private func open() {
        openRoute(FriendRoute(userId: friend.userId))
    }

    var body: some View {
        FriendCard(friend: friend, shared: terminals, open: open) {
            if people.isBusy(friend.handle) {
                Spinner(size: 14).transition(.opacity)
            } else {
                Disclosure().padding(.top, 4)
            }
        } footer: {
            EmptyView()
        }
        .accessibilityIdentifier("people.friend.\(friend.handle)")
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button { confirmingRemoval = true } label: {
                Label("Remove", systemImage: "person.badge.minus")
            }
            .tint(theme.colors.danger)
        }
        .swipeActions(edge: .leading, allowsFullSwipe: false) {
            if friend.identityChanged, !friend.inviteNeeded {
                Button { people.trust(friend) } label: {
                    Label("Trust", systemImage: "checkmark.shield")
                }
                .tint(theme.colors.caution)
            }
        }
        .contextMenu {
            Button { UIPasteboard.general.string = friend.handle } label: {
                Label("Copy username", systemImage: "doc.on.doc")
            }
            if friend.identityChanged, !friend.inviteNeeded {
                Button { people.trust(friend) } label: {
                    Label("Trust new identity", systemImage: "checkmark.shield")
                }
            }
            Divider()
            Button(role: .destructive) { confirmingRemoval = true } label: {
                Label("Remove…", systemImage: "person.badge.minus")
            }
        }
        .confirmationDialog(Text("Remove \(friend.personName)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
            Button("Remove", role: .destructive) { people.remove(friend) }
                .accessibilityIdentifier("people.confirmRemove")
        } message: {
            Text("You stop sharing terminals with each other, and each of you leaves the projects that the other owns. Kodosi does not undo the commands that they already ran.")
        }
    }
}

struct InviteShareButton<Content: View>: View {
    @Environment(AppModel.self) private var model
    let identifier: String
    @ViewBuilder let label: () -> Content

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        if let invite = people.ownInvite {
            ShareLink(item: invite, preview: SharePreview(Text("My Kodosi invite"))) {
                label()
            }
            .accessibilityIdentifier(identifier)
        } else {
            Button { people.requestInvite() } label: {
                label()
                    .opacity(people.fetchingInvite ? 0 : 1)
                    .overlay {
                        if people.fetchingInvite {
                            Spinner(size: 14)
                        }
                    }
            }
            .disabled(people.fetchingInvite)
            .accessibilityIdentifier(identifier)
        }
    }
}
