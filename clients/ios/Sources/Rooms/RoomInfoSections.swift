import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomMembersSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    let owner: Bool
    let remove: (MissionMemberEntry) -> Void

    var body: some View {
        let members = model.rooms.details[roomId]?.members
        Section {
            if let members {
                ForEach(members) { member in
                    row(member)
                }
            } else {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Loading people")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                }
            }
        } header: {
            Text(members.map { Counted.people($0.count) } ?? String(localized: "People"))
        }
        .listRowBackground(theme.colors.raised)
    }

    private func row(_ member: MissionMemberEntry) -> some View {
        let name = member.personName
        let me = member.userId == model.rooms.userId
        let removable = owner && !member.isOwner
        return HStack(spacing: 12) {
            PersonAvatar(name: name, key: member.userId, size: 34, isSelf: me)
            VStack(alignment: .leading, spacing: 1) {
                Text(me ? String(localized: "\(name) (you)") : name)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                Text(verbatim: "@\(member.handle)")
                    .appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkFaint)
            }
            Spacer(minLength: 8)
            if member.isOwner {
                Tag(text: String(localized: "Owner"))
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if removable {
                Button(role: .destructive) { remove(member) } label: {
                    Label("Remove", systemImage: "person.badge.minus")
                }
            }
        }
        .contextMenu {
            if removable {
                Button(role: .destructive) { remove(member) } label: {
                    Label("Remove from room", systemImage: "person.badge.minus")
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("rooms.member.\(AccessibilityIdentifier.token(member.userId))")
    }
}

struct RoomRepositoriesSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    @State private var address = ""
    @State private var connecting = false
    @State private var failure: String?
    @State private var connected = 0

    private var ready: Bool {
        !address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !connecting
    }

    var body: some View {
        let repositories = model.rooms.snapshots[roomId]?.repositories ?? []
        Section {
            ForEach(repositories) { repository in
                if let url = URL(string: repository.url), url.scheme?.lowercased() == "https" {
                    Link(destination: url) { row(repository) }
                        .accessibilityIdentifier("rooms.repository.\(AccessibilityIdentifier.token(repository.id))")
                } else {
                    row(repository)
                }
            }
            HStack(spacing: 8) {
                TextField("Paste a repository address", text: $address)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.go)
                    .onSubmit(connect)
                    .accessibilityIdentifier("rooms.repository.address")
                Button(connecting ? "Connecting…" : "Connect", action: connect)
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .disabled(!ready)
                    .accessibilityIdentifier("rooms.repository.connect")
            }
            if let failure {
                Text(failure)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.caution)
            }
        } header: {
            Text("Repositories")
        } footer: {
            Text("Kodosi on a computer turns their issues into room tasks. Your sign-in for them stays on that computer.")
        }
        .listRowBackground(theme.colors.raised)
        .sensoryFeedback(.success, trigger: connected)
    }

    private func row(_ repository: RoomRepository) -> some View {
        HStack(spacing: 12) {
            IconTile(symbol: "shippingbox.fill", tint: TileTint.graphite, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                Text(repository.name)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                Text(verbatim: repository.host)
                    .appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkFaint)
            }
            Spacer(minLength: 8)
            Image(systemName: "arrow.up.right")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.colors.inkFaint)
        }
    }

    private func connect() {
        guard ready else { return }
        connecting = true
        failure = nil
        let rooms = model.rooms
        let roomId = roomId
        let value = address
        Task {
            defer { connecting = false }
            do {
                try await rooms.addRepository(value, to: roomId)
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
