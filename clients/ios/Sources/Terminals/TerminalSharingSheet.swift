import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalSharingSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    @State private var projectId: String?
    @State private var people: Set<String> = []
    @State private var originalPeople: Set<String> = []
    @State private var saving = false
    @State private var failure: String?
    @State private var addingFriend = false

    var body: some View {
        NavigationStack {
            Group {
                if let session = model.terminals.session(sessionId) {
                    content(session)
                } else {
                    TerminalStateCard(symbol: "moon.zzz", title: String(localized: "This terminal has ended."), message: nil, working: false) {
                        EmptyView()
                    }
                }
            }
            .background(theme.colors.surface)
            .navigationTitle("Sharing")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Done", symbol: "xmark", identifier: "terminal.sharing.done", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.medium, .large])
        .kodosiSheet()
        .sheet(isPresented: $addingFriend) { AddFriendSheet() }
        .onAppear {
            let session = model.terminals.session(sessionId)
            projectId = session?.projectId
            originalPeople = Set(session?.sharedWith ?? [])
            people = originalPeople
        }
    }

    private func content(_ session: RuntimeSession) -> some View {
        let here = model.viewers(of: session)
        return ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(spacing: 11) {
                    AgentMark(session: session, size: 34, activity: session.mark(rested: .awake))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(session.isOwner ? String(localized: "Share \(session.name)") : session.name)
                            .appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                        Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                    }
                }
                if !here.isEmpty {
                    HStack(spacing: 8) {
                        AvatarStack(people: here, size: 22)
                        Text("Here now").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                }
                ListGroup {
                    ListRow(session.hostLabel, subtitle: hostState(session), symbol: DeviceGlyph.symbol(for: session.hostLabel),
                            tint: TileTint.graphite)
                    if let path = session.workingDir {
                        ListRow(HostPath.folderName(path), subtitle: path, monoSubtitle: true, symbol: "folder.fill", tint: TileTint.amber)
                    }
                    ListRow(access(session), symbol: "person.2.fill", tint: TileTint.blue)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("terminal.sharing.access")
                }
                if session.isOwner {
                    projects
                    friends(session)
                    if let failure {
                        ErrorNote(message: failure).transition(AnyTransition.rise)
                    }
                    Button { save(session) } label: {
                        Text(saving ? String(localized: "Sharing…") : actionTitle(session)).frame(maxWidth: .infinity).shimmer(saving)
                    }
                    .buttonStyle(.kodosi(.primary, size: .large))
                    .disabled(saving || !changed(session))
                    .accessibilityIdentifier("terminal.sharing.save")
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Everyone you share with can type, resize and close this terminal.")
                        if projectId != nil {
                            Text("Your friends in the project get full control, including friends who join later.")
                        }
                    }
                    .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                    .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(20)
            .frame(maxWidth: 640, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .animation(theme.motion.snappy, value: projectId)
        .animation(theme.motion.snappy, value: people)
        .animation(theme.motion.spring, value: failure)
    }

    private var projects: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("A project").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            if model.projects.entries.isEmpty {
                Text("You have no projects yet.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
            FlowLayout(spacing: 6) {
                ForEach(model.projects.entries) { project in
                    let selected = projectId == project.id
                    Button {
                        projectId = selected ? nil : project.id
                        if !selected {
                            people = originalPeople
                        }
                    } label: {
                        ProjectChoice(name: project.name, key: project.id, selected: selected)
                    }
                    .buttonStyle(PressScaleStyle())
                    .disabled(saving)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .accessibilityIdentifier("terminal.sharing.project.\(AccessibilityIdentifier.token(project.id))")
                }
            }
        }
    }

    @ContentBuilder
    private func friends(_ session: RuntimeSession) -> some View {
        if projectId == nil, session.projectId == nil {
            let friends = model.people.friends
            let offline = session.status == .offline
            VStack(alignment: .leading, spacing: 8) {
                Text("Friends").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                if offline {
                    Text("\(session.hostLabel) is offline. Share with friends when it is back.")
                        .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if friends.isEmpty {
                    Button { addingFriend = true } label: { Label("Add a friend", systemImage: "person.badge.plus") }
                        .buttonStyle(.kodosi(.secondary))
                        .accessibilityIdentifier("terminal.sharing.addFriend")
                } else {
                    FlowLayout(spacing: 10) {
                        ForEach(friends) { friend in friendChip(friend, offline: offline) }
                    }
                    ForEach(friends.filter { $0.identityChanged && !people.contains($0.userId) }) { friend in
                        ChangedIdentityNote(name: friend.personName)
                    }
                }
            }
            .transition(.opacity)
        }
    }

    private func friendChip(_ friend: FriendEntry, offline: Bool) -> some View {
        let name = friend.personName
        let selected = people.contains(friend.userId)
        let blocked = friend.identityChanged && !selected
        return Button {
            if selected {
                people.remove(friend.userId)
            } else {
                people.insert(friend.userId)
            }
        } label: {
            FriendChoice(name: name, key: friend.userId, selected: selected, changed: friend.identityChanged, blocked: blocked,
                         ring: theme.colors.surface)
        }
        .buttonStyle(PressScaleStyle())
        .disabled(saving || blocked || offline)
        .accessibilityLabel(Text(verbatim: "\(name) (@\(friend.handle))"))
        .accessibilityValue(friend.identityChanged ? Text("New identity") : Text(verbatim: ""))
        .accessibilityHint(blocked ? Text("Their identity changed. Trust them in Friends first.") : Text(verbatim: ""))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("terminal.sharing.friend.\(AccessibilityIdentifier.token(friend.userId))")
    }

    private func changed(_ session: RuntimeSession) -> Bool {
        projectId != session.projectId || people != originalPeople
    }

    private func actionTitle(_ session: RuntimeSession) -> String {
        if projectId != session.projectId {
            guard let projectId, let name = model.projects.project(projectId)?.name else { return String(localized: "Stop sharing") }
            return String(localized: "Share with project \(name)")
        }
        if !changed(session), projectId != nil || !people.isEmpty {
            return String(localized: "Shared")
        }
        if people.isEmpty {
            return originalPeople.isEmpty ? String(localized: "Share") : String(localized: "Stop sharing")
        }
        if people.count == 1, let id = people.first {
            return String(localized: "Share with \(model.personName(id))")
        }
        return String(localized: "Share with \(people.count) people")
    }

    private func hostState(_ session: RuntimeSession) -> String? {
        if session.status == .offline || model.unreachable(session) {
            return model.statusLabel(of: session)
        }
        return session.isConnected ? session.pathLabel : session.message ?? session.statusLabel
    }

    private func access(_ session: RuntimeSession) -> String {
        if !session.isOwner {
            let owner = session.ownerName ?? session.hostLabel
            return session.projectName.map { String(localized: "Shared by \(owner) in \($0)") } ?? String(localized: "Shared by \(owner)")
        }
        if let name = session.projectName {
            return String(localized: "Shared with project \(name)")
        }
        let names = model.sharedNames(of: session)
        return names.isEmpty ? String(localized: "Only you and your devices") : names.joined(separator: ", ")
    }

    private func save(_ session: RuntimeSession) {
        guard changed(session) else { return }
        saving = true
        failure = nil
        Task {
            do {
                if projectId != session.projectId {
                    try await model.terminals.share(session, inProject: projectId)
                } else {
                    try await model.terminals.share(session, with: people, expected: originalPeople)
                }
                dismiss()
            } catch RuntimeError.refused {
            } catch {
                failure = error.localizedDescription
            }
            saving = false
        }
    }
}
