import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct PeopleHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var adding = false

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        List {
            if !people.loaded {
                loadingSection
            } else if people.isEmpty, !people.sendingRequest {
                emptySection
            } else {
                requestsSection
                friendsSection
            }
        }
        .groupedListChrome()
        .navigationTitle("Friends")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                InviteShareButton(identifier: "people.shareInvite") {
                    Label("Share my invite", systemImage: "square.and.arrow.up")
                }
                Button { adding = true } label: {
                    Label("Add a friend", systemImage: "person.badge.plus")
                }
                .accessibilityIdentifier("people.add")
            }
        }
        .navigationDestination(for: FriendRoute.self) { FriendDetailView(userId: $0.userId) }
        .navigationDestination(for: TerminalRoute.self) { TerminalScreen(sessionId: $0.sessionId) }
        .navigationDestination(for: RoomRoute.self) { RoomScreen(roomId: $0.roomId) }
        .sheet(isPresented: $adding) { AddFriendSheet() }
        .refreshable { await people.refresh() }
        .task {
            if people.ownInvite == nil {
                people.requestInvite()
            }
        }
        .animation(theme.motion.spring, value: people.friends.map(\.id))
        .animation(theme.motion.spring, value: people.incoming.map(\.id))
        .animation(theme.motion.spring, value: people.outgoing.map(\.id))
        .animation(theme.motion.soft, value: people.loaded)
        .animation(theme.motion.snappy, value: people.sendingRequest)
        .sensoryFeedback(trigger: FriendTally(loaded: people.loaded, count: people.friends.count)) { old, new in
            old.loaded && new.count > old.count ? .success : nil
        }
        .sensoryFeedback(trigger: people.inFlight.count + (people.sendingRequest ? 1 : 0)) { old, new in
            new > old ? .impact(weight: .light) : nil
        }
    }

    private var loadingSection: some View {
        Section {
            ForEach(0 ..< 3, id: \.self) { index in
                HStack(spacing: 12) {
                    SkeletonBlock(width: 44, height: 44, radius: 22)
                    VStack(alignment: .leading, spacing: 8) {
                        SkeletonBlock(width: 140, height: 12)
                        SkeletonBlock(width: 90, height: 10)
                    }
                }
                .padding(.vertical, 6)
                .raisedRows()
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(Text("Loading friends"))
                .accessibilityHidden(index > 0)
            }
        }
    }

    private var emptySection: some View {
        EmptyState(
            title: "No friends yet",
            message: """
            Send your invite to a friend, or add them with their invite or username. \
            Friends can open the terminals you share, and you can open theirs.
            """,
        ) {
            TogetherArt(size: 52, ring: theme.colors.ground)
        } actions: {
            VStack(spacing: 10) { emptyActions }
                .frame(maxWidth: 300)
        }
        .padding(.top, 24)
        .bareRow()
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
    private var requestsSection: some View {
        if !people.incoming.isEmpty || !people.outgoing.isEmpty || people.sendingRequest {
            Section {
                ForEach(people.incoming) { request in
                    IncomingRequestRow(request: request)
                        .raisedRows(fill: theme.colors.raised.mix(with: theme.colors.accent, by: theme.isDark ? 0.1 : 0.07))
                }
                ForEach(people.outgoing) { request in
                    OutgoingRequestRow(request: request)
                        .raisedRows()
                }
                if people.sendingRequest {
                    ProgressCaption(text: String(localized: "Sending your request"))
                        .padding(.vertical, 8)
                        .raisedRows()
                        .accessibilityIdentifier("people.sending")
                }
            } header: {
                GroupedListHeader(title: "Requests", count: people.incoming.count)
            }
        }
    }

    @ViewBuilder
    private var friendsSection: some View {
        if !people.friends.isEmpty {
            Section {
                ForEach(people.friends) { friend in
                    FriendListItem(friend: friend, terminals: model.terminals.sessions.filter { $0.ownerUserId == friend.userId && !$0.hidden })
                }
            } header: {
                GroupedListHeader(title: "Friends", count: people.friends.count)
            }
        }
    }
}

private struct FriendTally: Equatable {
    let loaded: Bool
    let count: Int
}

private struct FriendListItem: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingRemoval = false
    let friend: FriendEntry
    let terminals: [RuntimeSession]

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        NavigationLink(value: FriendRoute(userId: friend.userId)) {
            FriendRow(friend: friend, terminals: terminals)
                .overlay(alignment: .trailing) {
                    if people.isBusy(friend.handle) {
                        ProgressView()
                            .padding(6)
                            .background(theme.colors.raised, in: Circle())
                            .transition(.opacity)
                    }
                }
        }
        .raisedRows()
        .accessibilityIdentifier("people.friend.\(friend.handle)")
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button { confirmingRemoval = true } label: {
                Label("Remove", systemImage: "person.badge.minus")
            }
            .tint(theme.colors.danger)
        }
        .swipeActions(edge: .leading, allowsFullSwipe: false) {
            if friend.identityChanged {
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
            if friend.identityChanged {
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
            Text("They lose the terminals you shared. Commands they already ran stay done.")
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
                            ProgressView()
                        }
                    }
            }
            .disabled(people.fetchingInvite)
            .accessibilityIdentifier(identifier)
        }
    }
}
