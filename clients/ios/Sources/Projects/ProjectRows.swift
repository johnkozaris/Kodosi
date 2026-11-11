import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectListCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let project: ProjectEntry
    let open: () -> Void

    var body: some View {
        let projects = model.projects
        let sessions = model.terminals.sessions.filter { $0.projectId == project.id }
        let last = projects.snapshots[project.id]?.messages.last
        ProjectCard(project: project, sessions: sessions, yours: projects.isOwner(of: project.id), open: open) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                preview(last)
                    .appTextStyle(.footnote)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if sessions.contains(where: \.needsUser) {
                    StatusSign(form: .hand)
                } else if let date = last.flatMap({ ProjectDates.parse($0.createdAt) }) {
                    Text(ProjectDates.short(date))
                        .appTextStyle(.caption)
                        .foregroundStyle(theme.colors.inkFaint)
                }
            }
            .padding(.top, 3)
        }
        .animation(theme.motion.snappy, value: project.unread)
    }

    @ViewBuilder
    private func preview(_ last: ProjectMessage?) -> some View {
        if let last {
            let author = model.projects.isOwn(last) ? String(localized: "You") : last.agent ?? last.authorName
            let text = String(ProjectMessageText.formatted(last.text).characters)
                .split(whereSeparator: \.isNewline).joined(separator: " ")
            Text("\(Text(verbatim: author).foregroundStyle(theme.colors.ink)): \(Text(verbatim: text))")
                .foregroundStyle(theme.colors.inkMuted)
        } else if model.projects.snapshots[project.id] != nil {
            Text("No messages yet").foregroundStyle(theme.colors.inkFaint)
        }
    }
}

struct ProjectInvitationRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let invitation: ProjectInvitationEntry
    let joined: (String) -> Void
    @State private var answering: Bool?
    @State private var accepted = 0

    var body: some View {
        let token = AccessibilityIdentifier.token(invitation.id)
        InvitationCard(invitation: invitation, joining: answering == true, identifier: "projects.invitation.\(token)") {
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
        .accessibilityIdentifier("projects.invitation.\(token)")
    }

    private func answer(_ accept: Bool) {
        guard answering == nil else { return }
        answering = accept
        Task {
            defer { answering = nil }
            do {
                try await model.projects.respond(to: invitation, accept: accept)
                if accept {
                    accepted += 1
                    joined(invitation.projectId)
                }
            } catch {
                model.projects.report(error)
            }
        }
    }
}
