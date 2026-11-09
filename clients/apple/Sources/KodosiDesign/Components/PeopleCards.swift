import KodosiBridge
import SwiftUI

/// A friend as a card: their face and name, and what they share with you. With `open`, the card is
/// a key; without it, the card lifts under the pointer and its marks open the terminals.
public struct FriendCard<Trailing: View, Footer: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let friend: FriendEntry
    let shared: [RuntimeSession]
    let open: (() -> Void)?
    let openTerminal: ((RuntimeSession) -> Void)?
    let trust: (() -> Void)?
    let verify: (() -> Void)?
    let trailing: Trailing
    let footer: Footer

    public init(
        friend: FriendEntry, shared: [RuntimeSession], open: (() -> Void)? = nil, openTerminal: ((RuntimeSession) -> Void)? = nil,
        trust: (() -> Void)? = nil, verify: (() -> Void)? = nil,
        @ViewBuilder trailing: () -> Trailing, @ViewBuilder footer: () -> Footer,
    ) {
        self.friend = friend
        self.shared = shared
        self.open = open
        self.openTerminal = openTerminal
        self.trust = trust
        self.verify = verify
        self.trailing = trailing()
        self.footer = footer()
    }

    public var body: some View {
        if let open {
            Button(action: open) { content }
                .buttonStyle(CardButtonStyle(radius: Radius.xl, rise: 0))
        } else {
            content
                .raised(Radius.xl, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
                .onHover { hovered = $0 }
                .animation(theme.motion.hover, value: hovered)
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 12) {
                PersonAvatar(name: friend.personName, key: friend.userId, size: 46)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 5) {
                        Text(friend.personName).appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                        if friend.isVerified {
                            VerifiedSeal(size: 12).help(Text("Verified", bundle: .module))
                        }
                    }
                    Text(verbatim: "@\(friend.handle)").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                }
                Spacer(minLength: 4)
                trailing
            }
            status.frame(minHeight: KodosiButtonStyle.Size.small.height)
            footer
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private var status: some View {
        if friend.identityChanged {
            HStack(spacing: 8) {
                Tag(text: String(localized: "New identity", bundle: .module), symbol: "exclamationmark", tone: .caution)
                Spacer()
                if friend.inviteNeeded {
                    if let verify {
                        Button(action: verify) { Text("Verify with their invite…", bundle: .module) }
                            .buttonStyle(.kodosi(.primary, size: .small))
                            .accessibilityIdentifier("people.verify.\(friend.handle)")
                    }
                } else if let trust {
                    Button(action: trust) { Text("Trust", bundle: .module) }
                        .buttonStyle(.kodosi(.primary, size: .small))
                        .accessibilityIdentifier("people.trust.\(friend.handle)")
                }
            }
        } else if shared.isEmpty {
            HStack {
                Text("Nothing shared with you yet", bundle: .module).appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                Spacer()
                if let verify, !friend.verified {
                    Button(action: verify) { Text("Verify", bundle: .module) }.buttonStyle(.kodosi(.ghost, size: .small))
                        .accessibilityIdentifier("people.verify.\(friend.handle)")
                }
            }
        } else {
            HStack(spacing: 6) {
                ForEach(shared.prefix(4)) { session in
                    if let openTerminal {
                        Button { openTerminal(session) } label: {
                            AgentMark(session: session, size: 24, activity: session.mark(rested: .awake))
                        }
                        .buttonStyle(PressScaleStyle()).help(session.name)
                    } else {
                        AgentMark(session: session, size: 24, activity: session.mark(rested: .awake))
                    }
                }
                Text("\(shared.count) shared with you", bundle: .module).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                Spacer()
            }
        }
    }
}

/// A person who asks to be your friend, with the keys to accept or not, and to block them with `block`.
public struct FriendRequestCard: View {
    @Environment(\.theme) private var theme
    let request: FriendRequestEntry
    let busy: Bool
    let identifier: String
    let decline: () -> Void
    let accept: () -> Void
    let block: (() -> Void)?

    public init(
        request: FriendRequestEntry, busy: Bool = false, identifier: String, decline: @escaping () -> Void, accept: @escaping () -> Void,
        block: (() -> Void)? = nil,
    ) {
        self.request = request
        self.busy = busy
        self.identifier = identifier
        self.decline = decline
        self.accept = accept
        self.block = block
    }

    public var body: some View {
        AnswerLayout {
            identity
        } keys: { stacked in
            if busy {
                Spinner(size: 16).frame(height: KodosiButtonStyle.Size.regular.height)
            } else if stacked {
                acceptKey(wide: true)
                declineKey(wide: true)
                blockKey(wide: true)
            } else {
                blockKey(wide: false)
                declineKey(wide: false)
                acceptKey(wide: false)
            }
        }
        .padding(14).frame(maxWidth: 620)
        .raised(Radius.xl)
        .overlay { RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(0.6), lineWidth: 1.5) }
    }

    private var identity: some View {
        HStack(spacing: 14) {
            PersonAvatar(name: request.personName, key: request.userId, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(request.personName) wants to be friends", bundle: .module).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                    .fixedSize(horizontal: false, vertical: true)
                Text(verbatim: "@\(request.handle)").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func declineKey(wide: Bool) -> some View {
        Button(action: decline) { Text("Not now", bundle: .module).frame(maxWidth: wide ? .infinity : nil) }
            .buttonStyle(.kodosi(.ghost))
            .accessibilityIdentifier("\(identifier).decline")
    }

    private func acceptKey(wide: Bool) -> some View {
        Button(action: accept) { Text("Accept…", bundle: .module).frame(maxWidth: wide ? .infinity : nil) }
            .buttonStyle(.kodosi(.primary))
            .accessibilityIdentifier("\(identifier).accept")
    }

    @ViewBuilder
    private func blockKey(wide: Bool) -> some View {
        if let block {
            Button(action: block) { Text("Block…", bundle: .module).frame(maxWidth: wide ? .infinity : nil) }
                .buttonStyle(.kodosi(.ghost))
                .accessibilityIdentifier("\(identifier).block")
        }
    }
}

/// A person you blocked, with the key to unblock them.
public struct BlockedCard: View {
    @Environment(\.theme) private var theme
    let person: BlockedEntry
    let busy: Bool
    let identifier: String
    let unblock: () -> Void

    public init(person: BlockedEntry, busy: Bool = false, identifier: String, unblock: @escaping () -> Void) {
        self.person = person
        self.busy = busy
        self.identifier = identifier
        self.unblock = unblock
    }

    public var body: some View {
        HStack(spacing: 12) {
            PersonAvatar(name: person.personName, key: person.userId, size: 28)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(person.personName).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                Text(verbatim: "@\(person.handle)").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
            }
            Spacer(minLength: 8)
            if busy {
                Spinner(size: 14)
            } else {
                Button(action: unblock) { Text("Unblock", bundle: .module) }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityIdentifier(identifier)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 8).frame(minHeight: 46).frame(maxWidth: 620)
        .well(Radius.lg)
    }
}

/// Your request to a person, while you wait for them.
public struct OutgoingRequestCard: View {
    @Environment(\.theme) private var theme
    let request: FriendRequestEntry
    let busy: Bool
    let identifier: String
    let cancel: () -> Void

    public init(request: FriendRequestEntry, busy: Bool = false, identifier: String, cancel: @escaping () -> Void) {
        self.request = request
        self.busy = busy
        self.identifier = identifier
        self.cancel = cancel
    }

    public var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) {
                avatar
                name
                tag
                Spacer()
                key
            }
            .frame(minHeight: 46)
            HStack(spacing: 12) {
                avatar
                identity
                Spacer()
                key
            }
            .padding(.vertical, 10)
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 12) {
                    avatar
                    identity
                }
                HStack {
                    Spacer()
                    key
                }
            }
            .padding(.vertical, 10)
        }
        .padding(.horizontal, 12).frame(maxWidth: 620)
        .well(Radius.lg)
    }

    private var avatar: some View {
        PersonAvatar(name: request.personName, key: request.userId, size: 28)
    }

    private var identity: some View {
        VStack(alignment: .leading, spacing: 4) {
            name
            tag
        }
    }

    private var name: some View {
        Text(request.personName).appTextStyle(.body).foregroundStyle(theme.colors.ink)
    }

    private var tag: some View {
        Tag(text: String(localized: "Waiting for them", bundle: .module))
    }

    @ViewBuilder
    private var key: some View {
        if busy {
            Spinner(size: 14)
        } else {
            Button(action: cancel) { Text("Cancel", bundle: .module) }
                .buttonStyle(.kodosi(.ghost, size: .small))
                .accessibilityLabel(Text("Cancel the request to \(request.personName)", bundle: .module))
                .accessibilityIdentifier(identifier)
        }
    }
}
