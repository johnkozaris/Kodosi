import KodosiBridge
import SwiftUI

public extension EnvironmentValues {
    /// The fill of the card under a view, for the rings of the badges on it.
    @Entry var cardFill: Color?
}

/// A raised card that is a button. It lifts while the pointer is on it, and on iPhone and iPad
/// while a finger presses it.
public struct CardButtonStyle: ButtonStyle {
    let radius: CGFloat
    let rise: CGFloat

    public init(radius: CGFloat, rise: CGFloat = 1) {
        self.radius = radius
        self.rise = rise
    }

    public func makeBody(configuration: Configuration) -> some View {
        CardFace(label: configuration.label, pressed: configuration.isPressed, radius: radius, rise: rise)
    }
}

private struct CardFace<Label: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let label: Label
    let pressed: Bool
    let radius: CGFloat
    let rise: CGFloat

    private var lit: Bool {
        #if os(iOS)
            hovered || pressed
        #else
            hovered
        #endif
    }

    var body: some View {
        let fill = lit ? theme.colors.lifted : theme.colors.raised
        label
            .environment(\.cardFill, fill)
            .raised(radius, fill: fill, elevation: lit ? .lifted : .resting)
            .offset(y: lit ? -rise : 0)
            .environment(\.bezelPressed, pressed)
            .scaleEffect(pressed ? 0.98 : 1)
            .animation(theme.motion.snappy, value: pressed)
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
    }
}

public extension RuntimeSession {
    /// Whether the terminal is in a room or shared with people.
    var isShared: Bool {
        missionId != nil || isOwner && !sharedWith.isEmpty
    }
}

/// A terminal as a card: its mark, its name and a line about it, and at the end what it needs.
public struct TerminalCard<Detail: View, Accessory: View>: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let rested: AgentMark.Activity
    let badged: Bool
    let muted: Bool
    let open: () -> Void
    let detail: Detail
    let accessory: Accessory

    /// With `badged`, a terminal in a room or shared with people has a small square on its mark.
    /// A `muted` name is for a terminal that does not open now.
    public init(
        session: RuntimeSession, rested: AgentMark.Activity, badged: Bool = false, muted: Bool = false, open: @escaping () -> Void,
        @ViewBuilder detail: () -> Detail, @ViewBuilder accessory: () -> Accessory,
    ) {
        self.session = session
        self.rested = rested
        self.badged = badged
        self.muted = muted
        self.open = open
        self.detail = detail()
        self.accessory = accessory()
    }

    public var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                AgentMark(session: session, size: 34, activity: session.mark(rested: rested))
                    .overlay(alignment: .bottomTrailing) {
                        if badged, session.isShared {
                            CardShareBadge(room: session.missionId).offset(x: 4, y: 4)
                        }
                    }
                VStack(alignment: .leading, spacing: 3) {
                    Text(session.name).appTextStyle(.subhead).foregroundStyle(muted ? theme.colors.inkMuted : theme.colors.ink).lineLimit(1)
                    detail.appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer(minLength: 0)
                accessory
            }
            .padding(.horizontal, 14)
            #if os(iOS)
                .padding(.vertical, 10).frame(minHeight: 68)
            #else
                .frame(height: 68)
            #endif
        }
        .buttonStyle(CardButtonStyle(radius: Radius.lg))
    }
}

private struct CardShareBadge: View {
    @Environment(\.theme) private var theme
    @Environment(\.cardFill) private var fill
    let room: String?

    var body: some View {
        ShareBadge(room: room, ring: fill ?? theme.colors.raised)
    }
}

/// What a terminal does now: why it does not connect, or its activity, or else where it runs.
public struct TerminalCaption: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let fallback: String?

    public init(session: RuntimeSession, fallback: String? = nil) {
        self.session = session
        self.fallback = fallback
    }

    public var body: some View {
        HStack(spacing: 5) {
            if session.isTroubled || session.connectionState == .connecting {
                Text(session.statusLabel)
                    .foregroundStyle(session.isTroubled ? theme.colors.caution : theme.colors.inkFaint)
                    .shimmer(session.connectionState == .connecting)
            } else if session.status == .reconnecting {
                Text("Reconnecting", bundle: .module).foregroundStyle(theme.colors.caution).shimmer()
            } else if session.status == .closing {
                Text(session.statusLabel).foregroundStyle(theme.colors.inkFaint)
            } else {
                SessionActivityText(session: session, fallback: fallback, showsPlace: true)
                    .foregroundStyle(session.activity == nil ? theme.colors.inkFaint : theme.colors.inkMuted)
            }
        }
        .lineLimit(1)
        .animation(theme.motion.soft, value: session.activity)
        .animation(theme.motion.snappy, value: session.progress)
    }
}

/// A room as a card: its mark, its name and the marks of its terminals. A copper dot says that
/// someone wrote in it since you looked, and a tag says that it is yours.
public struct RoomCard<Detail: View>: View {
    @Environment(\.theme) private var theme
    let mission: MissionEntry
    let sessions: [RuntimeSession]
    let yours: Bool
    let open: () -> Void
    let detail: Detail

    public init(
        mission: MissionEntry, sessions: [RuntimeSession], yours: Bool, open: @escaping () -> Void, @ViewBuilder detail: () -> Detail,
    ) {
        self.mission = mission
        self.sessions = sessions
        self.yours = yours
        self.open = open
        self.detail = detail()
    }

    public var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top) {
                    RoomSigil(key: mission.id, size: 44)
                    Spacer()
                    if mission.unread {
                        NewDot().padding(4)
                    } else if yours {
                        Tag(text: String(localized: "Yours", bundle: .module))
                    }
                }
                Spacer(minLength: 14)
                Text(mission.name).appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                detail
                HStack(spacing: 6) {
                    if sessions.isEmpty {
                        Text("No terminals yet", bundle: .module).appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                    } else {
                        HStack(spacing: -4) {
                            ForEach(sessions.prefix(5)) { session in
                                AgentMark(session: session, size: 18, activity: session.mark(rested: .awake))
                            }
                        }
                        Text(sessions.contains(where: \.working) ? String(localized: "Working", bundle: .module) : Counted.terminals(sessions.count))
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                }
                .padding(.top, 6).controlHeight(24)
            }
            .padding(16).frame(maxWidth: .infinity, alignment: .leading)
            #if os(iOS)
                .frame(minHeight: 132)
            #else
                .frame(height: 132)
            #endif
        }
        .buttonStyle(CardButtonStyle(radius: Radius.xl, rise: 2))
        .accessibilityValue(mission.unread ? Text("New messages", bundle: .module) : Text(verbatim: ""))
    }
}

/// An invitation to a room, with the keys to join it or not.
public struct InvitationCard: View {
    @Environment(\.theme) private var theme
    let invitation: MissionInvitationEntry
    let joining: Bool
    let identifier: String
    let decline: () -> Void
    let join: () -> Void

    public init(
        invitation: MissionInvitationEntry, joining: Bool = false, identifier: String, decline: @escaping () -> Void,
        join: @escaping () -> Void,
    ) {
        self.invitation = invitation
        self.joining = joining
        self.identifier = identifier
        self.decline = decline
        self.join = join
    }

    public var body: some View {
        AnswerLayout {
            identity
        } keys: { stacked in
            if stacked {
                joinKey(wide: true)
                declineKey(wide: true)
            } else {
                declineKey(wide: false)
                joinKey(wide: false)
            }
        }
        .padding(14)
        .raised(Radius.xl)
        .overlay { RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(0.6), lineWidth: 1.5) }
    }

    private var identity: some View {
        HStack(spacing: 14) {
            RoomSigil(key: invitation.missionId, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(invitation.missionName).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text("\(invitation.inviterName) invited you", bundle: .module).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
        }
    }

    private func declineKey(wide: Bool) -> some View {
        Button(action: decline) { Text("Not now", bundle: .module).frame(maxWidth: wide ? .infinity : nil) }
            .buttonStyle(.kodosi(.ghost))
            .accessibilityIdentifier("\(identifier).decline")
    }

    private func joinKey(wide: Bool) -> some View {
        Button(action: join) {
            Text(joining ? "Joining…" : "Join", bundle: .module).shimmer(joining).frame(maxWidth: wide ? .infinity : nil)
        }
        .buttonStyle(.kodosi(.primary))
        .accessibilityIdentifier("\(identifier).join")
    }
}

/// Who asks and the keys to answer: in one row when they fit, else the keys under who asks, else
/// the keys under each other at the width of the card.
struct AnswerLayout<Identity: View, Keys: View>: View {
    let identity: Identity
    let keys: (Bool) -> Keys

    init(@ViewBuilder identity: () -> Identity, @ViewBuilder keys: @escaping (_ stacked: Bool) -> Keys) {
        self.identity = identity()
        self.keys = keys
    }

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 14) {
                identity
                Spacer()
                keys(false)
            }
            VStack(alignment: .leading, spacing: 12) {
                identity.frame(idealWidth: 0, maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 8) {
                    Spacer()
                    keys(false)
                }
            }
            VStack(alignment: .leading, spacing: 12) {
                identity
                VStack(spacing: 8) { keys(true) }
            }
        }
    }
}
