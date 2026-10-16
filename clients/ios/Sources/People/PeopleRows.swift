import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct FriendRoute: Hashable {
    let userId: String
}

struct FriendRow: View {
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var typeSize
    let friend: FriendEntry
    let terminals: [RuntimeSession]

    var body: some View {
        HStack(spacing: 12) {
            PersonAvatar(name: friend.personName, key: friend.userId, size: 44)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 5) {
                    Text(friend.personName)
                        .appTextStyle(.headline)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(typeSize.isAccessibilitySize ? 2 : 1)
                    if friend.isVerified {
                        VerifiedSeal(size: 13)
                            .transition(AnyTransition.pop)
                    }
                }
                if friend.identityChanged {
                    Tag(text: String(localized: "New identity"), symbol: "exclamationmark", tone: .caution)
                        .transition(.opacity)
                } else {
                    Text(verbatim: "@\(friend.handle)")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                        .lineLimit(1)
                }
                if typeSize.isAccessibilitySize, !terminals.isEmpty {
                    TerminalMarkStack(sessions: terminals)
                        .padding(.top, 4)
                }
            }
            Spacer(minLength: 8)
            if !typeSize.isAccessibilitySize, !terminals.isEmpty {
                TerminalMarkStack(sessions: terminals)
            }
        }
        .padding(.vertical, 4)
        .animation(theme.motion.spring, value: friend)
        .accessibilityElement(children: .combine)
    }
}

struct TerminalMarkStack: View {
    @Environment(\.theme) private var theme
    let sessions: [RuntimeSession]
    var size: CGFloat = 24
    var limit = 3

    var body: some View {
        HStack(spacing: -size * 0.22) {
            ForEach(sessions.prefix(limit)) { session in
                AgentMark(kind: session.agent, size: size, activity: session.mark(rested: session.isConnected ? .awake : .asleep))
                    .padding(2)
                    .background(theme.colors.raised, in: RoundedRectangle(cornerRadius: size * 0.36, style: .continuous))
                    .transition(AnyTransition.pop)
            }
            if sessions.count > limit {
                Text(verbatim: "+\(sessions.count - limit)")
                    .font(.system(size: size * 0.42, weight: .semibold, design: .rounded))
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(.leading, size * 0.36)
            }
        }
        .animation(theme.motion.snappy, value: sessions.map(\.id))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("\(sessions.count) shared with you"))
    }
}

struct IncomingRequestRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let request: FriendRequestEntry

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) {
                identity
                Spacer(minLength: 8)
                actions
            }
            VStack(alignment: .leading, spacing: 12) {
                identity
                actions.frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
        .padding(.vertical, 6)
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            Button { people.accept(request) } label: {
                Label("Accept", systemImage: "checkmark")
            }
            .tint(theme.colors.accent)
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button { people.decline(request) } label: {
                Label("Not now", systemImage: "xmark")
            }
            .tint(TileTint.graphite)
        }
        .contextMenu {
            Button { people.accept(request) } label: {
                Label("Accept", systemImage: "checkmark")
            }
            Button { people.decline(request) } label: {
                Label("Not now", systemImage: "xmark")
            }
        }
    }

    private var identity: some View {
        HStack(spacing: 12) {
            PersonAvatar(name: request.personName, key: request.userId, size: 40)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(request.personName) wants to be friends")
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                    .fixedSize(horizontal: false, vertical: true)
                Text(verbatim: "@\(request.handle)")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
            }
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var actions: some View {
        if people.isBusy(request.handle) {
            ProgressView()
                .frame(height: KodosiButtonStyle.Size.small.height)
        } else {
            HStack(spacing: 8) {
                Button("Not now") { people.decline(request) }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityIdentifier("people.request.decline.\(request.handle)")
                Button("Accept") { people.accept(request) }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .accessibilityIdentifier("people.request.accept.\(request.handle)")
            }
            .fixedSize()
        }
    }
}

struct OutgoingRequestRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var typeSize
    let request: FriendRequestEntry

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        Group {
            if typeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 8) {
                    identity
                    cancel
                }
            } else {
                HStack(spacing: 12) {
                    identity
                    Spacer(minLength: 8)
                    cancel
                }
            }
        }
        .padding(.vertical, 4)
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button { people.cancel(request) } label: {
                Label("Cancel request", systemImage: "xmark")
            }
            .tint(TileTint.graphite)
        }
        .contextMenu {
            Button { people.cancel(request) } label: {
                Label("Cancel request", systemImage: "xmark")
            }
            Button { UIPasteboard.general.string = request.handle } label: {
                Label("Copy username", systemImage: "doc.on.doc")
            }
        }
    }

    private var identity: some View {
        HStack(spacing: 12) {
            PersonAvatar(name: request.personName, key: request.userId, size: 36)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(request.personName)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(typeSize.isAccessibilitySize ? 2 : 1)
                Tag(text: String(localized: "Waiting for them"))
            }
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var cancel: some View {
        if people.isBusy(request.handle) {
            ProgressView()
        } else {
            Button("Cancel") { people.cancel(request) }
                .buttonStyle(.kodosi(.ghost, size: .small))
                .accessibilityLabel(Text("Cancel the request to \(request.personName)"))
                .accessibilityIdentifier("people.request.cancel.\(request.handle)")
        }
    }
}
