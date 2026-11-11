import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectConversationView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let projectId: String
    let detail: ProjectDetail
    @Bindable var state: ProjectViewState
    let workspace: ProjectWorkspace
    @State private var copied = false

    private var messages: [ProjectMessage] {
        deps.projects.snapshots[projectId]?.messages ?? []
    }

    private var targets: [ProjectMention] {
        ProjectMentions.targets(members: detail.members, sessions: deps.projectSessions(projectId), userId: deps.account.userId)
    }

    var body: some View {
        let targets = targets
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text("Conversation").appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                Spacer()
                Button(action: copyInvite) {
                    Label(copied ? String(localized: "Copied. Paste it to your agent") : String(localized: "Bring an agent"),
                          systemImage: copied ? "checkmark" : "sparkles")
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(.kodosi(copied ? .tinted : .ghost, size: .small))
                .help("Copy an invite that tells an agent how to join this project")
                .accessibilityLabel(Text("Copy agent instructions")).accessibilityIdentifier("project.agent.instructions")
                .onHover { inside in
                    if !inside {
                        copied = false
                    }
                }
            }
            .padding(.leading, 16).padding(.trailing, 8).frame(height: 46)
            if deps.projects.snapshots[projectId] == nil {
                if let failure = state.readFailure, !state.loading {
                    ProjectRecovery(message: failure) { Task { await deps.projects.read(projectId) } }
                } else {
                    ProjectSkeleton()
                }
            } else {
                transcript(targets)
                ProjectComposer(projectId: projectId, state: state, targets: targets)
            }
        }
        .raised(Radius.xl, fill: theme.colors.sheet)
        .showsProjectPart(.conversation, of: state)
        .animation(theme.motion.snappy, value: copied)
        .environment(\.openURL, OpenURLAction { url in
            guard let id = ProjectMentions.terminalId(in: url) else { return .systemAction }
            if deps.session(id) != nil {
                workspace.shownSection = 0
                workspace.selectedTerminal = id
                deps.activateSession(id, inProject: projectId)
            }
            return .handled
        })
    }

    private func transcript(_ targets: [ProjectMention]) -> some View {
        let messages = messages
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if deps.projects.snapshots[projectId]?.hasOlder == true, let first = messages.first {
                        ProjectEarlierKey(loading: state.loading, identifier: "project.messages.earlier") {
                            Task { await deps.projects.read(projectId, before: first.sequence); proxy.scrollTo(first.id, anchor: .top) }
                        }
                        .padding(.bottom, 10)
                    } else if deps.projects.snapshots[projectId]?.historyTrimmed == true, !messages.isEmpty {
                        ProjectTrimmedNote().padding(.bottom, 10)
                    }
                    ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                        let previous = index > 0 ? messages[index - 1] : nil
                        ProjectMessageRow(
                            projectId: projectId, message: message, previous: previous, targets: targets, state: state, workspace: workspace,
                        )
                        .id(message.id)
                    }
                    Color.clear.frame(height: 1).id("latest")
                }
                .scrollTargetLayout().padding(.horizontal, 14).padding(.vertical, 10)
            }
            .defaultScrollAnchor(.bottom)
            .scrollPosition(id: $state.readingMessage, anchor: .bottom)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 36
            } action: { _, latest in
                state.followsLatest = latest
            }
            .overlay {
                if messages.isEmpty {
                    ProjectWelcome(name: detail.project.name, key: projectId, steps: [
                        ProjectWelcome.Step(title: "Invite people", symbol: "person.badge.plus", tint: TileTint.blue,
                                            done: detail.members.count > 1) { workspace.peopleVisible = true },
                        ProjectWelcome.Step(title: "Add a terminal", symbol: "apple.terminal", tint: TileTint.graphite,
                                            done: !deps.projectSessions(projectId).isEmpty) { deps.openLauncher(projectId: projectId) },
                        ProjectWelcome.Step(title: copied ? "Copied. Paste it to your agent" : "Bring an agent in", symbol: "sparkles",
                                            tint: TileTint.orange, done: copied, action: copyInvite),
                    ])
                }
            }
            .overlay(alignment: .bottom) {
                if !state.followsLatest {
                    ProjectLatestKey(unread: state.unread, identifier: "project.messages.latest") {
                        withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                    }
                    .padding(10)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(theme.motion.spring, value: state.followsLatest)
            .animation(theme.motion.spring, value: messages.last?.id)
            .onChange(of: messages.last?.id) { _, _ in
                if state.followsLatest {
                    withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                }
            }
            .onChange(of: state.followsLatest) { _, follows in
                if follows {
                    withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                }
            }
        }
    }

    private func copyInvite() {
        let path = (Bundle.main.executablePath ?? "kodosi").replacingOccurrences(of: "'", with: "'\"'\"'")
        deps.copyText("""
        Read `'\(path)' skill`, then use \
        `'\(path)' --json project --project \(projectId) context`.
        Use this executable for the skill's kodosi commands. You can read and post updates, \
        create and pick up tasks, and share results for this work.
        """)
        copied = true
    }
}
