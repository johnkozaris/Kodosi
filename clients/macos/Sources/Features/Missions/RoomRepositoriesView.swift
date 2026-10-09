import SwiftUI

struct RoomRepositoriesView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Namespace private var chip
    let roomId: String
    @Bindable var state: RoomViewState
    @State private var loadingIssues = false
    @State private var issueFailure: String?
    @FocusState private var connecting: Bool

    private var repositories: [RoomRepository] {
        deps.rooms[roomId]?.repositories ?? []
    }

    private var selected: RoomRepository? {
        repositories.first { $0.id == state.selectedRepository }
    }

    var body: some View {
        VStack(spacing: 0) {
            if repositories.isEmpty {
                EmptyState(title: "Connect a repository", message: "Its issues can become room tasks. Your sign-in for it stays on this Mac.") {
                    IconTile(symbol: "shippingbox.fill", tint: TileTint.purple, size: 52)
                } actions: {
                    connectField.frame(width: 380)
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
                    if state.showsRepositoryForm {
                        connectField.frame(width: 320).transition(.opacity.combined(with: .move(edge: .trailing)))
                    } else {
                        IconButton(title: "Connect a repository", symbol: "plus", identifier: "room.repository.new", size: 30) {
                            withAnimation(theme.motion.spring) { state.showsRepositoryForm = true }
                            connecting = true
                        }
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
    }

    private func repositoryChip(_ repository: RoomRepository) -> some View {
        let active = state.selectedRepository == repository.id
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
        .buttonStyle(.plain).help("\(repository.host)/\(repository.owner)/\(repository.repository)")
        .accessibilityAddTraits(active ? .isSelected : [])
        .accessibilityIdentifier("room.repository.\(AccessibilityIdentifier.token(repository.id))")
    }

    private var connectField: some View {
        HStack(spacing: 6) {
            Image(systemName: "link").font(.system(size: 11, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
            TextField("Paste a repository address", text: $state.repositoryURL)
                .textFieldStyle(.plain).appTextStyle(.body).focused($connecting)
                .onSubmit(connect).onExitCommand { state.showsRepositoryForm = false }
                .accessibilityIdentifier("room.repository.url")
            Button("Connect", action: connect)
                .buttonStyle(.kodosi(.primary, size: .small)).disabled(state.busy || state.repositoryURL.isEmpty)
                .accessibilityIdentifier("room.repository.add")
        }
        .padding(.leading, 12).padding(.trailing, 4).frame(height: 34)
        .wellCapsule()
    }

    private func issues(_ repository: RoomRepository) -> some View {
        let entries = deps.roomIssues[repository.id] ?? []
        return ScrollView {
            LazyVStack(spacing: 6) {
                ForEach(entries) { issue in
                    let imported = deps.rooms[roomId]?.tasks.contains { $0.issue?.url == issue.url } == true
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
                            perform(["type": .string("importIssue"), "repositoryId": .string(repository.id), "number": .uint(issue.number)])
                        } label: {
                            Label(imported ? String(localized: "In tasks") : String(localized: "Add to tasks"),
                                  systemImage: imported ? "checkmark" : "plus")
                                .contentTransition(.symbolEffect(.replace))
                        }
                        .buttonStyle(.kodosi(imported ? .ghost : .tinted, size: .small)).disabled(imported || state.busy)
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

    private func connect() {
        guard !state.repositoryURL.isEmpty, !state.busy else { return }
        let revision = state.repositoryRevision
        perform(["type": .string("addRepository"), "url": .string(state.repositoryURL)]) {
            if state.repositoryRevision == revision {
                state.repositoryURL = ""; state.showsRepositoryForm = false
            }
        }
    }

    private func select(_ repository: RoomRepository) {
        state.selectedRepository = repository.id
        issueFailure = nil; loadingIssues = true
        Task { @MainActor in
            do {
                try await deps.roomAction(roomId, ["type": .string("issues"), "repositoryId": .string(repository.id)])
            } catch {
                if state.selectedRepository == repository.id {
                    issueFailure = error.localizedDescription
                }
            }
            if state.selectedRepository == repository.id {
                loadingIssues = false
            }
        }
    }

    private func perform(_ action: [String: JSONValue], completed: @escaping @MainActor () -> Void = {}) {
        state.busy = true; state.failure = nil
        Task { @MainActor in
            defer { state.busy = false }
            do {
                try await deps.roomAction(roomId, action)
                completed()
            } catch { state.failure = error.localizedDescription }
        }
    }
}
