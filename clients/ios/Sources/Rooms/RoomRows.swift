import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let mission: MissionEntry

    var body: some View {
        let rooms = model.rooms
        let last = rooms.snapshots[mission.id]?.messages.last
        let unread = mission.unread
        HStack(alignment: .top, spacing: 12) {
            RoomSigil(key: mission.id, size: 46)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(mission.name)
                        .appTextStyle(.headline)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    if let date = last.flatMap({ RoomDates.parse($0.createdAt) }) {
                        Text(RoomDates.short(date))
                            .appTextStyle(.caption)
                            .foregroundStyle(unread ? theme.colors.accentStrong : theme.colors.inkFaint)
                    }
                }
                HStack(alignment: .top, spacing: 8) {
                    preview(last)
                        .appTextStyle(.footnote)
                        .lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if unread {
                        NewDot().padding(.top, 4).transition(AnyTransition.pop)
                    }
                }
                RoomRowActivity(mission: mission)
            }
        }
        .padding(.vertical, 6)
        .animation(theme.motion.snappy, value: unread)
        .accessibilityElement(children: .combine)
        .accessibilityValue(unread ? Text("New messages") : Text(verbatim: ""))
    }

    @ViewBuilder
    private func preview(_ last: RoomMessage?) -> some View {
        if let last {
            let author = model.rooms.isOwn(last) ? String(localized: "You") : last.agent ?? last.authorName
            let text = String(RoomMessageText.formatted(last.text).characters)
                .split(whereSeparator: \.isNewline).joined(separator: " ")
            Text("\(Text(verbatim: author).foregroundStyle(theme.colors.ink)): \(Text(verbatim: text))")
                .foregroundStyle(theme.colors.inkMuted)
        } else if model.rooms.snapshots[mission.id] != nil {
            Text("No messages yet").foregroundStyle(theme.colors.inkFaint)
        } else {
            Text(model.rooms.isOwner(of: mission.id) ? "Your room" : "Shared with you").foregroundStyle(theme.colors.inkFaint)
        }
    }
}

private struct RoomRowActivity: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let mission: MissionEntry

    var body: some View {
        let members = model.rooms.details[mission.id]?.members ?? []
        let terminals = model.terminals.sessions.filter { $0.missionId == mission.id }
        let working = terminals.contains(where: \.isWorking)
        let asking = terminals.contains(where: \.needsUser)
        if !members.isEmpty || !terminals.isEmpty {
            HStack(spacing: 8) {
                if !members.isEmpty {
                    AvatarStack(
                        people: members.map { member in
                            AvatarStack.Person(id: member.userId, name: member.personName, isSelf: member.userId == model.rooms.userId)
                        },
                        size: 20, limit: 4, ring: theme.colors.raised,
                    )
                }
                Spacer(minLength: 0)
                if !terminals.isEmpty {
                    HStack(spacing: -5) {
                        ForEach(terminals.prefix(4)) { session in
                            AgentMark(session: session, size: 18, activity: session.mark(rested: .awake))
                        }
                    }
                    if asking {
                        StatusSign(form: .hand, size: 11)
                    }
                    Text(working ? String(localized: "Working") : Counted.terminals(terminals.count))
                        .appTextStyle(.caption)
                        .foregroundStyle(theme.colors.inkMuted)
                }
            }
            .padding(.top, 4)
            .animation(theme.motion.spring, value: terminals.map(\.id))
        }
    }
}

struct RoomInvitationRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let invitation: MissionInvitationEntry
    let joined: (String) -> Void
    @State private var answering: Bool?
    @State private var accepted = 0

    var body: some View {
        let token = AccessibilityIdentifier.token(invitation.id)
        HStack(alignment: .top, spacing: 12) {
            RoomSigil(key: invitation.missionId, size: 46)
            VStack(alignment: .leading, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(invitation.missionName)
                        .appTextStyle(.headline)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(2)
                    Text("\(invitation.inviterName) invited you")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                }
                HStack(spacing: 8) {
                    Button { answer(true) } label: {
                        Text(answering == true ? "Joining…" : "Join").shimmer(answering == true)
                    }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .accessibilityIdentifier("rooms.invitation.\(token).join")
                    Button("Not now") { answer(false) }
                        .buttonStyle(.kodosi(.ghost, size: .small))
                        .accessibilityIdentifier("rooms.invitation.\(token).decline")
                }
                .disabled(answering != nil)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 6)
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            Button { answer(true) } label: { Label("Join", systemImage: "checkmark") }
                .tint(theme.colors.accent)
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button { answer(false) } label: { Label("Not now", systemImage: "xmark") }
                .tint(theme.colors.inkFaint)
        }
        .sensoryFeedback(.success, trigger: accepted)
        .accessibilityIdentifier("rooms.invitation.\(token)")
    }

    private func answer(_ accept: Bool) {
        guard answering == nil else { return }
        answering = accept
        Task {
            defer { answering = nil }
            do {
                try await model.rooms.respond(to: invitation, accept: accept)
                if accept {
                    accepted += 1
                    joined(invitation.missionId)
                }
            } catch {
                model.rooms.report(error)
            }
        }
    }
}
