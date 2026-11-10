import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomRepositoriesView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Namespace private var chip
    let roomId: String
    let state: RoomViewState
    @Bindable var workspace: RoomWorkspace
    @State private var loadingIssues = false
    @State private var issueFailure: String?

    private var repositories: [RoomRepository] {
        deps.rooms.snapshots[roomId]?.repositories ?? []
    }

    private var selected: RoomRepository? {
        repositories.first { $0.id == workspace.selectedRepository }
    }

    var body: some View {
        VStack(spacing: 0) {
            if repositories.isEmpty {
                ScrollView {
                    VStack(spacing: 14) {
                        IconTile(symbol: "shippingbox.fill", tint: TileTint.purple, size: 52)
                        Text("Link a repository").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                        RepositoryPicker(roomId: roomId)
                            .padding(12).raised(Radius.xl)
                    }
                    .frame(width: 440).padding(.top, 48).frame(maxWidth: .infinity)
                }
            } else {
                HStack(spacing: 8) {
                    ScrollView(.horizontal) {
                        HStack(spacing: 2) {
                            ForEach(repositories) { repository in repositoryChip(repository) }
                        }
                        .padding(3)
                    }
                    .scrollIndicators(.never).wellCapsule().fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                    IconButton(title: "Link a repository", symbol: "plus", identifier: "room.repository.new", size: 30,
                               open: workspace.showsRepositoryPicker)
                    {
                        workspace.showsRepositoryPicker = true
                    }
                    .kodosiPopover(isPresented: $workspace.showsRepositoryPicker, alignment: .trailing) {
                        RepositoryPicker(roomId: roomId) { workspace.showsRepositoryPicker = false }
                            .padding(12).frame(width: 400)
                    }
                }
                .padding(.horizontal, 8).padding(.bottom, 6)
                if loadingIssues {
                    RoomSkeleton()
                } else if let issueFailure {
                    RoomRecovery(message: issueFailure) {
                        if let selected {
                            select(selected)
                        }
                    }
                } else if let selected {
                    issues(selected)
                } else {
                    Spacer()
                }
            }
        }
        .onAppear {
            if let repository = selected ?? repositories.first {
                select(repository)
            }
        }
        .onChange(of: repositories.map(\.id)) { _, _ in
            if selected == nil, let repository = repositories.first {
                select(repository)
            }
        }
        .showsRoomPart(.repositories, of: state)
    }

    private func repositoryChip(_ repository: RoomRepository) -> some View {
        let active = workspace.selectedRepository == repository.id
        return Button { withAnimation(theme.motion.snappy) { select(repository) } } label: {
            HStack(spacing: 6) {
                Image(systemName: "shippingbox").font(.system(size: 10, weight: .semibold))
                Text(repository.name).lineLimit(1)
            }
            .appTextStyle(.footnote).fontWeight(.medium)
            .foregroundStyle(active ? theme.colors.ink : theme.colors.inkMuted)
            .padding(.horizontal, 11).frame(height: 28)
            .background {
                if active {
                    RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                        .matchedGeometryEffect(id: "chip", in: chip)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain).help(Text(verbatim: "\(repository.host)/\(repository.owner)/\(repository.repository)"))
        .accessibilityAddTraits(active ? .isSelected : [])
        .accessibilityIdentifier("room.repository.\(AccessibilityIdentifier.token(repository.id))")
    }

    private func issues(_ repository: RoomRepository) -> some View {
        let entries = deps.rooms.issues[repository.id] ?? []
        return ScrollView {
            LazyVStack(spacing: 6) {
                ForEach(entries) { issue in
                    let imported = deps.rooms.snapshots[roomId]?.tasks.contains { $0.issue?.url == issue.url } == true
                    HStack(alignment: .center, spacing: 12) {
                        Image(systemName: issue.closed ? "checkmark.circle.fill" : "smallcircle.filled.circle")
                            .font(.system(size: 15)).foregroundStyle(issue.closed ? theme.colors.inkFaint : theme.colors.ready)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(issue.title).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(2)
                            Text(verbatim: "#\(issue.number)" + (issue.assignees.isEmpty ? "" : " · " + issue.assignees.joined(separator: ", ")))
                                .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        Button {
                            perform(RoomAction.importIssue(repositoryId: repository.id, number: issue.number))
                        } label: {
                            Label(imported ? String(localized: "In tasks") : String(localized: "Add to tasks"),
                                  systemImage: imported ? "checkmark" : "plus")
                                .contentTransition(.symbolEffect(.replace))
                        }
                        .buttonStyle(.kodosi(imported ? .ghost : .tinted, size: .small)).disabled(imported || workspace.busy)
                        .accessibilityLabel(imported ? Text("Added to tasks") : Text("Add \(issue.title) to tasks"))
                    }
                    .padding(.horizontal, 14).padding(.vertical, 11)
                    .raised(Radius.lg)
                }
                if entries.isEmpty {
                    Text("No open issues.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).padding(40)
                }
            }
            .padding(.horizontal, 18).padding(.top, 6).padding(.bottom, 24)
            .frame(maxWidth: 760).frame(maxWidth: .infinity)
        }
    }

    private func select(_ repository: RoomRepository) {
        workspace.selectedRepository = repository.id
        issueFailure = nil; loadingIssues = true
        Task { @MainActor in
            do {
                try await deps.rooms.act(roomId, RoomAction.issues(repository.id))
            } catch {
                if workspace.selectedRepository == repository.id {
                    issueFailure = error.localizedDescription
                }
            }
            if workspace.selectedRepository == repository.id {
                loadingIssues = false
            }
        }
    }

    private func perform(_ action: [String: JSONValue], completed: @escaping @MainActor () -> Void = {}) {
        workspace.busy = true; state.failure = nil
        Task { @MainActor in
            defer { workspace.busy = false }
            do {
                try await deps.rooms.act(roomId, action)
                completed()
            } catch { state.failure = error.localizedDescription }
        }
    }
}
