import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectMembersSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let projectId: String
    let owner: Bool
    let remove: (ProjectMemberEntry) -> Void
    let block: (ProjectMemberEntry) -> Void
    let report: (ProjectMemberEntry) -> Void

    var body: some View {
        let members = model.projects.details[projectId]?.members
        ListGroup(title: "\(members.map { Counted.people($0.count) } ?? String(localized: "People"))") {
            if let members {
                ForEach(members) { member in
                    row(member)
                }
            } else {
                HStack(spacing: 10) {
                    Spinner(size: 14)
                    Text("Loading people")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                }
                .padding(14)
            }
        }
    }

    private func row(_ member: ProjectMemberEntry) -> some View {
        let me = member.userId == model.projects.userId
        let removable = owner && !member.isOwner
        return ListRow(title: member.listedName(you: model.projects.userId), subtitle: "@\(member.handle)") {
            PersonAvatar(name: member.personName, key: member.userId, size: 28, isSelf: me)
        } trailing: {
            if member.isOwner {
                Tag(text: String(localized: "Owner"))
            }
        }
        .contentShape(Rectangle())
        .contextMenu {
            if removable {
                Button(role: .destructive) { remove(member) } label: {
                    Label("Remove from project", systemImage: "person.badge.minus")
                }
            }
            if !me {
                Button(role: .destructive) { block(member) } label: {
                    Label("Block…", systemImage: "hand.raised")
                }
                Button(role: .destructive) { report(member) } label: {
                    Label("Report…", systemImage: "flag")
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("projects.member.\(AccessibilityIdentifier.token(member.userId))")
    }
}

struct ProjectRepositoriesSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @ScaledMetric(relativeTo: .body) private var arrow: CGFloat = 12
    let projectId: String
    @State private var address = ""
    @State private var connecting = false
    @State private var failure: String?
    @State private var connected = 0

    private var ready: Bool {
        !address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !connecting
    }

    var body: some View {
        let repositories = model.projects.snapshots[projectId]?.repositories ?? []
        ListGroup(title: "Repositories",
                  footer: "Kodosi on a computer turns their issues into project tasks. Your sign-in for them stays on that computer.")
        {
            ForEach(repositories) { repository in
                if let url = URL(string: repository.url), url.scheme?.lowercased() == "https" {
                    Link(destination: url) { row(repository) }
                        .buttonStyle(ListRowButtonStyle())
                        .accessibilityIdentifier("projects.repository.\(AccessibilityIdentifier.token(repository.id))")
                } else {
                    row(repository)
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    TextField("Paste a repository address", text: $address)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .submitLabel(.go)
                        .onSubmit(connect)
                        .accessibilityIdentifier("projects.repository.address")
                    Button(connecting ? "Connecting…" : "Connect", action: connect)
                        .buttonStyle(.kodosi(.primary))
                        .disabled(!ready)
                        .accessibilityIdentifier("projects.repository.connect")
                }
                if let failure {
                    Text(failure)
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.caution)
                }
            }
            .padding(12)
        }
        .sensoryFeedback(.success, trigger: connected)
    }

    private func row(_ repository: ProjectRepository) -> some View {
        ListRow(repository.name, subtitle: repository.host, symbol: "shippingbox.fill", tint: TileTint.purple) {
            Image(systemName: "arrow.up.right")
                .font(.system(size: arrow, weight: .semibold))
                .foregroundStyle(theme.colors.inkFaint)
        }
    }

    private func connect() {
        guard ready else { return }
        connecting = true
        failure = nil
        let projects = model.projects
        let projectId = projectId
        let value = address
        Task {
            defer { connecting = false }
            do {
                try await projects.addRepository(value, to: projectId)
                if address == value {
                    address = ""
                }
                connected += 1
            } catch is CancellationError {
                return
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}
