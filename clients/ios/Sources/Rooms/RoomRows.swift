import KodosiBridge
import KodosiDesign
import SwiftUI

/// A room in the list: its card, with the last message and its time.
struct RoomListCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let mission: MissionEntry
    let open: () -> Void

    var body: some View {
        let rooms = model.rooms
        let sessions = model.terminals.sessions.filter { $0.missionId == mission.id }
        let last = rooms.snapshots[mission.id]?.messages.last
        RoomCard(mission: mission, sessions: sessions, yours: rooms.isOwner(of: mission.id), open: open) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                preview(last)
                    .appTextStyle(.footnote)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if sessions.contains(where: \.needsUser) {
                    StatusSign(form: .hand)
                } else if let date = last.flatMap({ RoomDates.parse($0.createdAt) }) {
                    Text(RoomDates.short(date))
                        .appTextStyle(.caption)
                        .foregroundStyle(theme.colors.inkFaint)
                }
            }
            .padding(.top, 3)
        }
        .animation(theme.motion.snappy, value: mission.unread)
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
        }
    }
}

/// An invitation to a room in the list, with the keys to join it or not.
struct RoomInvitationRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let invitation: MissionInvitationEntry
    let joined: (String) -> Void
    @State private var answering: Bool?
    @State private var accepted = 0

    var body: some View {
        let token = AccessibilityIdentifier.token(invitation.id)
        InvitationCard(invitation: invitation, joining: answering == true, identifier: "rooms.invitation.\(token)") {
            answer(false)
        } join: {
            answer(true)
        }
        .disabled(answering != nil)
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            Button { answer(true) } label: { Label("Join", systemImage: "checkmark") }
                .tint(theme.colors.accent)
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button { answer(false) } label: { Label("Not now", systemImage: "xmark") }
                .tint(TileTint.graphite)
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
