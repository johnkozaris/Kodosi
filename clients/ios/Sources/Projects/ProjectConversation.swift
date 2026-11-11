import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectConversation: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let projectId: String
    @State private var position = ScrollPosition(edge: .bottom)
    @State private var touching = false

    private var projects: ProjectStore {
        model.projects
    }

    private var targets: [ProjectMention] {
        ProjectMentions.targets(
            members: projects.details[projectId]?.members ?? [],
            sessions: model.terminals.sessions.filter { $0.projectId == projectId },
            userId: projects.userId,
        )
    }

    var body: some View {
        let state = projects.view(projectId)
        let targets = targets
        Group {
            if let snapshot = projects.snapshots[projectId] {
                transcript(snapshot, state: state, targets: targets)
                    .safeAreaInset(edge: .bottom) { ProjectComposer(projectId: projectId, targets: targets) }
            } else if let failure = state.readFailure, !state.loading {
                ProjectRecovery(message: failure) { Task { await projects.read(projectId) } }
            } else {
                ProjectSkeleton()
            }
        }
        .showsProjectPart(.conversation, of: state)
        .onAppear {
            if !state.followsLatest, let reading = state.readingMessage {
                position.scrollTo(id: reading, anchor: .top)
            }
        }
        .onDisappear {
            state.readingMessage = state.followsLatest ? nil : position.viewID(type: String.self)
        }
    }

    private func transcript(_ snapshot: ProjectSnapshot, state: ProjectViewState, targets: [ProjectMention]) -> some View {
        let messages = snapshot.messages
        let unread = state.unread
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if snapshot.hasOlder, let first = messages.first {
                    ProjectEarlierKey(loading: state.loading, identifier: "projects.messages.earlier") {
                        Task {
                            await projects.read(projectId, before: first.sequence)
                            position.scrollTo(id: first.id, anchor: .top)
                        }
                    }
                    .padding(.bottom, 8)
                } else if snapshot.historyTrimmed, !messages.isEmpty {
                    ProjectTrimmedNote().padding(.bottom, 8)
                }
                ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                    ProjectMessageRow(
                        projectId: projectId,
                        message: message,
                        previous: index > 0 ? messages[index - 1] : nil,
                        next: index + 1 < messages.count ? messages[index + 1] : nil,
                        targets: targets,
                    )
                    .transition(AnyTransition.rise)
                }
            }
            .scrollTargetLayout()
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 8)
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
            .animation(theme.motion.spring, value: messages.last?.id)
        }
        .scrollPosition($position)
        .defaultScrollAnchor(.bottom, for: .initialOffset)
        .scrollDismissesKeyboard(.interactively)
        .onScrollPhaseChange { _, phase in
            touching = phase == .interacting || phase == .decelerating
        }
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentOffset.y + geometry.contentInsets.top + geometry.containerSize.height >= geometry.contentSize.height - 40
        } action: { _, bottom in
            if bottom {
                state.followsLatest = true
            } else if touching {
                state.followsLatest = false
            }
        }
        .onScrollGeometryChange(for: [CGFloat].self) { geometry in
            [geometry.contentSize.height, geometry.containerSize.height]
        } action: { old, new in
            if state.followsLatest, old != new {
                position.scrollTo(edge: .bottom)
            }
        }
        .overlay {
            if messages.isEmpty {
                welcome()
            }
        }
        .overlay(alignment: .bottom) {
            if !state.followsLatest {
                ProjectLatestKey(unread: unread, identifier: "projects.messages.latest") {
                    state.followsLatest = true
                    withAnimation(theme.motion.soft) { position.scrollTo(edge: .bottom) }
                }
                .padding(.bottom, 10)
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(theme.motion.spring, value: state.followsLatest)
        .animation(theme.motion.snappy, value: unread)
    }

    private func welcome() -> some View {
        let screen = model.projectScreen(projectId)
        var steps: [ProjectWelcome.Step] = []
        if projects.isOwner(of: projectId) {
            steps.append(ProjectWelcome.Step(
                title: "Invite people", symbol: "person.badge.plus", tint: TileTint.blue,
                done: (projects.details[projectId]?.members.count ?? 0) > 1,
            ) { screen.showsInfo = true })
        }
        steps.append(ProjectWelcome.Step(
            title: "Add a terminal", symbol: "apple.terminal", tint: TileTint.graphite,
            done: model.terminals.sessions.contains { $0.projectId == projectId },
        ) { show(.terminals, in: screen) })
        steps.append(ProjectWelcome.Step(
            title: "Put up a task", symbol: "checklist", tint: TileTint.orange,
            done: !(projects.snapshots[projectId]?.tasks.isEmpty ?? true),
        ) { show(.tasks, in: screen) })
        return ProjectWelcome(name: projects.project(projectId)?.name ?? "", key: projectId, steps: steps)
    }

    private func show(_ section: ProjectSection, in screen: ProjectScreenState) {
        screen.section = section
        screen.shownSection = section
    }
}
