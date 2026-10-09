import AppKit
import SwiftUI

struct PeopleView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var username = ""
    @State private var pendingRemoval: FriendEntry?
    @FocusState private var adding: Bool

    private var facts: String {
        let waiting = deps.incomingRequests.count
        let friends = Counted.friends(deps.friends.count)
        return waiting > 0 ? friends + " · " + String(localized: "\(waiting) waiting") : friends
    }

    var body: some View {
        if !deps.accountReady {
            SignInView(title: "Add the people you work with", message: "Friends can open the terminals you share, and you can open theirs.")
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    PageHeader(title: "People", facts: facts) {
                        Button { deps.perform("friends.invite") } label: {
                            Label(deps.ownInvite == nil ? String(localized: "Copy my invite") : String(localized: "Invite copied"),
                                  systemImage: deps.ownInvite == nil ? "link" : "checkmark")
                                .contentTransition(.symbolEffect(.replace))
                        }
                        .buttonStyle(.kodosi(deps.ownInvite == nil ? .secondary : .tinted))
                        .accessibilityIdentifier("people.copyInvite")
                    }.arrive()
                    addCapsule.arrive(delay: 0.04)
                    if !deps.incomingRequests.isEmpty || !deps.outgoingRequests.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            ForEach(deps.incomingRequests) { request in incoming(request).transition(AnyTransition.rise) }
                            ForEach(deps.outgoingRequests) { request in outgoing(request).transition(AnyTransition.rise) }
                        }
                    }
                    if deps.friends.isEmpty {
                        EmptyState(title: "No friends yet", message: "Send your invite, or paste theirs above.") {
                            TogetherArt()
                        } actions: { EmptyView() }
                            .frame(height: 260)
                    } else {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 260, maximum: 360), spacing: 14)], alignment: .leading, spacing: 14) {
                            ForEach(Array(deps.friends.enumerated()), id: \.element.id) { index, friend in
                                FriendCard(friend: friend) { pendingRemoval = friend }
                                    .arrive(delay: 0.04 * Double(min(index, 8) + 2))
                            }
                        }
                    }
                }
                .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 36)
                .frame(maxWidth: 1100, alignment: .leading).frame(maxWidth: .infinity)
            }
            .animation(theme.motion.spring, value: deps.friends.map(\.id))
            .animation(theme.motion.spring, value: deps.incomingRequests.map(\.id))
            .animation(theme.motion.spring, value: deps.outgoingRequests.map(\.id))
            .animation(theme.motion.snappy, value: deps.ownInvite)
            .confirmationDialog(
                "Remove \(pendingRemoval?.displayName ?? "")?",
                isPresented: Binding(get: { pendingRemoval != nil }, set: {
                    if !$0 {
                        pendingRemoval = nil
                    }
                }), titleVisibility: .visible
            ) {
                if let friend = pendingRemoval {
                    Button("Remove", role: .destructive) {
                        deps.perform("friends.remove", ["username": .string(friend.handle)])
                        pendingRemoval = nil
                    }
                }
            } message: { Text("They lose the terminals you shared. Commands they already ran stay done.") }
            .onChange(of: deps.ownInvite) { _, text in
                guard let text else { return }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
            }
            .onDisappear { deps.ownInvite = nil }
        }
    }

    private var addCapsule: some View {
        let trimmed = username.trimmingCharacters(in: .whitespacesAndNewlines)
        return HStack(spacing: 10) {
            Image(systemName: "person.badge.plus").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(adding ? theme.colors.accentStrong : theme.colors.inkFaint)
            TextField("Paste a friend’s invite, or type a username", text: $username)
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
        deps.perform("friends.request.send", ["username": .string(name), "requestId": .string(UUIDv7.generate())])
        username = ""
    }

    private func incoming(_ person: FriendRequestEntry) -> some View {
        let name = person.displayName.isEmpty ? person.handle : person.displayName
        return HStack(spacing: 14) {
            PersonAvatar(name: name, key: person.userId, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(name) wants to be friends").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text(verbatim: "@\(person.handle)").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
            Spacer()
            Button("Not now") { deps.perform("friends.request.reject", ["username": .string(person.handle)]) }.buttonStyle(.kodosi(.ghost))
            Button("Accept") { deps.perform("friends.request.accept", ["username": .string(person.handle)]) }.buttonStyle(.kodosi(.primary))
        }
        .padding(14).frame(maxWidth: 620)
        .raised(Radius.xl)
        .overlay { RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(0.6), lineWidth: 1.5) }
    }

    private func outgoing(_ person: FriendRequestEntry) -> some View {
        let name = person.displayName.isEmpty ? person.handle : person.displayName
        return HStack(spacing: 12) {
            PersonAvatar(name: name, key: person.userId, size: 28)
            Text(name).appTextStyle(.body).foregroundStyle(theme.colors.ink)
            Tag(text: String(localized: "Waiting for them"))
            Spacer()
            Button("Cancel") { deps.perform("friends.request.cancel", ["username": .string(person.handle)]) }
                .buttonStyle(.kodosi(.ghost, size: .small))
        }
        .padding(.horizontal, 12).frame(height: 46).frame(maxWidth: 620)
        .well(Radius.lg)
    }
}

private struct FriendCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    @State private var verifying = false
    @State private var invite = ""
    let friend: FriendEntry
    let remove: () -> Void

    private var name: String {
        friend.displayName.isEmpty ? friend.handle : friend.displayName
    }

    private var shared: [RuntimeSession] {
        deps.sessions.filter { $0.ownerUserId == friend.userId }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 12) {
                PersonAvatar(name: name, key: friend.userId, size: 46)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 5) {
                        Text(name).appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                        if friend.verified, !friend.identityChanged {
                            Image(systemName: "checkmark.seal.fill").font(.system(size: 12)).foregroundStyle(theme.colors.ready)
                                .help("Verified").accessibilityLabel(Text("Verified"))
                        }
                    }
                    Text(verbatim: "@\(friend.handle)").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                }
                Spacer(minLength: 4)
                Menu {
                    if !friend.verified || friend.identityChanged {
                        Button("Verify with their invite…") { verifying = true }
                    }
                    Button("Remove…", role: .destructive, action: remove)
                } label: {
                    Image(systemName: "ellipsis").font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                        .frame(width: 26, height: 26).contentShape(Circle())
                }
                .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
                .accessibilityLabel(Text("Options for \(name)"))
            }
            if friend.identityChanged {
                HStack(spacing: 8) {
                    Tag(text: String(localized: "New identity"), symbol: "exclamationmark", tone: .caution)
                    Spacer()
                    Button("Trust") { deps.perform("friends.identity.trust", ["username": .string(friend.handle)]) }
                        .buttonStyle(.kodosi(.primary, size: .small))
                        .accessibilityIdentifier("people.trust.\(friend.handle)")
                }
            } else if shared.isEmpty {
                HStack {
                    Text("Nothing shared with you yet").appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                    Spacer()
                    if !friend.verified {
                        Button("Verify") { verifying.toggle() }.buttonStyle(.kodosi(.ghost, size: .small))
                            .accessibilityIdentifier("people.verify.\(friend.handle)")
                    }
                }
            } else {
                HStack(spacing: 6) {
                    ForEach(shared.prefix(4)) { session in
                        Button { deps.activateSession(session.id) } label: {
                            AgentMark(kind: session.agent, size: 24, activity: session.mark(rested: .awake))
                        }
                        .buttonStyle(PressScaleStyle()).help(session.name)
                    }
                    Text("\(shared.count) shared with you").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    Spacer()
                }
            }
            if verifying {
                HStack(spacing: 6) {
                    TextField("Paste the invite \(name) sent you", text: $invite)
                        .accessibilityIdentifier("people.verifyInvite")
                    Button("Verify") {
                        deps.perform("friends.verify", [
                            "username": .string(friend.handle),
                            "invite": .string(invite.trimmingCharacters(in: .whitespacesAndNewlines)),
                        ])
                        verifying = false; invite = ""
                    }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .disabled(invite.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.xl, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.spring, value: verifying)
    }
}
