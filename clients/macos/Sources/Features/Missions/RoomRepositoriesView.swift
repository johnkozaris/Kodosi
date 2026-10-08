import SwiftUI

struct RoomRepositoriesView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @Bindable var state: RoomViewState
    @State private var loadingIssues = false
    @State private var issueFailure: String?
    private var repositories: [RoomRepository] {
        deps.rooms[roomId]?.repositories ?? []
    }

    private var selected: RoomRepository? {
        repositories.first { $0.id == state.selectedRepository }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                if let selected {
                    Text(selected.name).font(.system(size: 14, weight: .semibold)).lineLimit(1)
                }
                Spacer()
                Button { state.showsRepositoryForm.toggle() } label: { Label("Connect repository", systemImage: "plus") }
                    .buttonStyle(SolidPrimaryButtonStyle()).popover(isPresented: $state.showsRepositoryForm) { repositoryForm }
                    .accessibilityIdentifier("room.repository.new")
            }.padding(16)
            if repositories.isEmpty {
                VStack(spacing: 16) {
                    Image(systemName: "point.3.connected.trianglepath.dotted").font(.system(size: 44, weight: .ultraLight))
                        .foregroundStyle(theme.colors.primary)
                    Button("Connect repository") { state.showsRepositoryForm = true }
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView(.horizontal) {
                    HStack(spacing: 8) {
                        ForEach(repositories) { repository in
                            Button { select(repository) } label: {
                                HStack(spacing: 6) {
                                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                                    Text(repository.name).lineLimit(1)
                                }.font(.system(size: 12, weight: .medium)).padding(10)
                                    .background(
                                        state.selectedRepository == repository.id ? theme.colors.secondary : .clear,
                                        in: RoundedRectangle(cornerRadius: 9)
                                    )
                            }.buttonStyle(.plain).help(repository.host)
                                .accessibilityIdentifier("room.repository.\(AccessibilityIdentifier.token(repository.id))")
                        }
                    }.padding(.horizontal, 16)
                }.scrollIndicators(.hidden)
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
        }.onAppear {
            if let repository = selected ?? repositories.first {
                select(repository)
            }
        }
    }

    private func issues(_ repository: RoomRepository) -> some View {
        let entries = deps.roomIssues[repository.id] ?? []
        return ScrollView {
            LazyVStack(spacing: 0) {
                ForEach(entries) { issue in
                    let imported = deps.rooms[roomId]?.tasks.contains { $0.issue?.url == issue.url } == true
                    HStack(alignment: .center, spacing: 12) {
                        Image(systemName: issue.closed ? "checkmark.circle" : "circle.dotted").foregroundStyle(theme.colors.tertiary)
                        VStack(alignment: .leading, spacing: 5) {
                            Text(issue.title).font(.system(size: 13, weight: .medium)).lineLimit(2)
                            Text("#\(issue.number)").font(.system(size: 11)).foregroundStyle(theme.colors.mutedForeground)
                        }.frame(maxWidth: .infinity, alignment: .leading)
                        Button { perform(["type": .string("importIssue"), "repositoryId": .string(repository.id), "number": .uint(issue.number)])
                        } label: {
                            Image(systemName: imported ? "checkmark" : "plus").contentTransition(.symbolEffect(.replace))
                        }.buttonStyle(.plain).disabled(imported || state.busy).help(imported ? "Added to tasks" : "Add to tasks")
                            .accessibilityLabel(imported ? Text("Added to tasks") : Text("Add \(issue.title) to tasks"))
                    }.padding(.vertical, 14)
                    Divider().opacity(0.55)
                }
                if entries.isEmpty {
                    Image(systemName: "tray").font(.system(size: 36, weight: .ultraLight)).foregroundStyle(theme.colors.mutedForeground).padding(40)
                }
            }.padding(.horizontal, 18)
        }
    }

    private var repositoryForm: some View {
        VStack(alignment: .leading, spacing: 14) {
            TextField("Repository URL", text: $state.repositoryURL).accessibilityIdentifier("room.repository.url")
            HStack {
                Button("Cancel") { state.showsRepositoryForm = false }
                Spacer()
                Button("Connect") {
                    let revision = state.repositoryRevision
                    perform(["type": .string("addRepository"), "url": .string(state.repositoryURL)]) {
                        if state.repositoryRevision == revision {
                            state.repositoryURL = ""; state.showsRepositoryForm = false
                        }
                    }
                }.buttonStyle(SolidPrimaryButtonStyle()).disabled(state.busy || state.repositoryURL.isEmpty)
                    .accessibilityIdentifier("room.repository.add")
            }
        }.padding(20).frame(width: 380)
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
