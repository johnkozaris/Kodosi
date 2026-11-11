import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct ProjectTaskSelection: Identifiable {
    let id: String
}

struct ProjectTasks: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let projectId: String
    @State private var composing = false
    @State private var opened: ProjectTaskSelection?
    @ScaledMetric(relativeTo: .callout) private var plus: CGFloat = 12

    var body: some View {
        let projects = model.projects
        let state = projects.view(projectId)
        Group {
            if let snapshot = projects.snapshots[projectId] {
                list(snapshot, state: state)
            } else if let failure = state.readFailure, !state.loading {
                ProjectRecovery(message: failure) { Task { await projects.read(projectId) } }
            } else {
                ProjectSkeleton()
            }
        }
        .showsProjectPart(.tasks, of: state)
        .sheet(isPresented: $composing) { ProjectTaskComposer(projectId: projectId) }
        .sheet(item: $opened) { ProjectTaskSheet(projectId: projectId, taskId: $0.id) }
        .sensoryFeedback(.success, trigger: model.projectScreen(projectId).finishedTasks)
    }

    private func list(_ snapshot: ProjectSnapshot, state: ProjectViewState) -> some View {
        let lanes = ProjectTaskLanes(snapshot.tasks, repository: state.repositoryFilter)
        return List {
            header(snapshot.repositories, state: state)
                .cardRow(top: 10, bottom: 6)
            if lanes.tasks.isEmpty {
                EmptyState(title: "No tasks yet", message: "Put one up. People and agents can pick it up.") {
                    ProgressRing(value: 0, size: 44, lineWidth: 5)
                } actions: {
                    EmptyView()
                }
                .frame(minHeight: 240)
                .cardRow()
            }
            lane(.upForGrabs, tasks: lanes.upForGrabs)
            lane(.inProgress, tasks: lanes.inProgress)
            done(lanes.done, state: state)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .readableWidth()
        .refreshable { await model.projects.read(projectId) }
        .animation(theme.motion.spring, value: lanes.tasks.map { "\($0.id):\($0.version)" })
        .animation(theme.motion.spring, value: state.showsDone)
    }

    private func header(_ repositories: [ProjectRepository], state: ProjectViewState) -> some View {
        HStack(spacing: 10) {
            Button { composing = true } label: {
                HStack(spacing: 10) {
                    Image(systemName: "plus").font(.system(size: plus, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
                    Text("Add a task").appTextStyle(.callout).foregroundStyle(theme.colors.inkFaint)
                    Spacer(minLength: 0)
                }
                .frame(minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("projects.tasks.new")
            if !repositories.isEmpty {
                Menu {
                    Picker("Repository", selection: Binding(get: { state.repositoryFilter }, set: { state.repositoryFilter = $0 })) {
                        Text("All repositories").tag(String?.none)
                        ForEach(repositories) { repository in
                            Text(repository.name).tag(String?.some(repository.id))
                        }
                    }
                } label: {
                    Label(
                        repositories.first { $0.id == state.repositoryFilter }?.name ?? String(localized: "All"),
                        systemImage: "line.3.horizontal.decrease",
                    )
                    .lineLimit(1)
                }
                .menuStyle(.button)
                .buttonStyle(.kodosi(.ghost, size: .small))
                .fixedSize()
                .accessibilityIdentifier("projects.tasks.filter")
            }
        }
        .padding(.leading, 14).padding(.trailing, 6)
        .raised(Radius.xl)
    }

    @ViewBuilder
    private func lane(_ lane: ProjectTaskLaneTitle.Lane, tasks: [ProjectTask]) -> some View {
        if !tasks.isEmpty {
            ProjectTaskLaneTitle(lane, count: tasks.count)
                .cardRow(top: 16, bottom: 2)
            ForEach(tasks) { task in
                ProjectTaskRow(projectId: projectId, task: task) { opened = ProjectTaskSelection(id: task.id) }
            }
        }
    }

    @ViewBuilder
    private func done(_ tasks: [ProjectTask], state: ProjectViewState) -> some View {
        if !tasks.isEmpty {
            ProjectDoneTasksKey(state: state, count: tasks.count, identifier: "projects.tasks.done.toggle")
                .cardRow(top: 12, bottom: 2)
            if state.showsDone {
                ForEach(tasks) { task in
                    ProjectTaskRow(projectId: projectId, task: task) { opened = ProjectTaskSelection(id: task.id) }
                }
            }
        }
    }
}

struct ProjectTaskRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let projectId: String
    let task: ProjectTask
    let open: () -> Void
    @State private var pending: ProjectTaskChange?
    @ScaledMetric(relativeTo: .body) private var checkSize: CGFloat = 20

    private var token: String {
        AccessibilityIdentifier.token(task.id)
    }

    private var linked: Bool {
        task.issue != nil
    }

    private var mine: Bool {
        task.assignedTo != nil && task.assignedTo == model.projects.userId
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            check
            Button(action: open) {
                VStack(alignment: .leading, spacing: 6) {
                    ProjectTaskTitle(task: task, lines: 3)
                    ProjectTaskChips(
                        task: task, repositories: model.projects.snapshots[projectId]?.repositories ?? [],
                        terminal: task.terminalId.flatMap { model.terminals.session($0) },
                    )
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("projects.task.\(token)")
            trailing
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .wash(trigger: task.version)
        .raised(Radius.lg)
        .projectTaskMarker(mine && !task.closed)
        .cardRow()
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            if !linked {
                Button { change(task.closed ? .reopen : .close) } label: {
                    Label(task.closed ? "Reopen" : "Complete", systemImage: task.closed ? "arrow.uturn.backward" : "checkmark")
                }
                .tint(task.closed ? TileTint.graphite : theme.colors.ready)
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            if !linked, !task.closed {
                if task.assignedTo == nil {
                    Button { change(.claim) } label: { Label("Pick up", systemImage: "hand.raised") }
                        .tint(theme.colors.accent)
                } else {
                    Button { change(.release) } label: { Label("Release", systemImage: "arrow.uturn.left") }
                        .tint(TileTint.graphite)
                }
            }
        }
        .contextMenu { menu }
    }

    @ViewBuilder
    private var check: some View {
        let closed = pending.map { $0 == .close } ?? task.closed
        if linked {
            Image(systemName: task.closed ? "checkmark.circle.fill" : "smallcircle.filled.circle")
                .font(.system(size: checkSize, weight: .regular))
                .foregroundStyle(task.closed ? theme.colors.ready : theme.colors.inkFaint)
                .accessibilityLabel(task.closed ? Text("Done") : Text("Open"))
        } else {
            Button { change(task.closed ? .reopen : .close) } label: {
                Image(systemName: closed ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: checkSize, weight: .regular))
                    .foregroundStyle(closed && pending == nil ? theme.colors.ready : theme.colors.inkFaint)
                    .contentTransition(.symbolEffect(.replace))
                    .touchTarget()
            }
            .buttonStyle(PressScaleStyle())
            .disabled(pending != nil)
            .accessibilityLabel(task.closed ? Text("Reopen \(task.title)") : Text("Complete \(task.title)"))
            .accessibilityIdentifier("projects.task.\(token).complete")
        }
    }

    @ViewBuilder
    private var trailing: some View {
        if let person = task.assignedName {
            PersonAvatar(name: person, key: task.assignedTo, size: 24, isSelf: mine)
                .accessibilityLabel(Text("Picked up by \(person)"))
        } else if !task.closed, !linked {
            Button { change(.claim) } label: {
                Text(pending == .claim ? "Picking up…" : "Pick up").shimmer(pending == .claim)
            }
            .buttonStyle(.kodosi(.tinted, size: .small))
            .disabled(pending != nil)
            .accessibilityLabel(Text("Pick up \(task.title)"))
            .accessibilityIdentifier("projects.task.\(token).claim")
        }
    }

    @ViewBuilder
    private var menu: some View {
        if !linked {
            if task.closed {
                Button { change(.reopen) } label: { Label("Reopen", systemImage: "arrow.uturn.backward.circle") }
            } else {
                if task.assignedTo == nil {
                    Button { change(.claim) } label: { Label("Pick up", systemImage: "hand.raised") }
                } else {
                    Button { change(.release) } label: { Label("Release", systemImage: "arrow.uturn.left") }
                }
                Button { change(.close) } label: { Label("Complete", systemImage: "checkmark.circle") }
            }
        }
        if let issue = task.issue, let url = issue.address {
            Link(destination: url) { Label("Open issue #\(issue.number)", systemImage: "arrow.up.right") }
        }
        Button { UIPasteboard.general.string = task.title } label: { Label("Copy title", systemImage: "doc.on.doc") }
    }

    private func change(_ change: ProjectTaskChange) {
        guard pending == nil else { return }
        pending = change
        let projects = model.projects
        let screen = model.projectScreen(projectId)
        let projectId = projectId
        let task = task
        Task {
            defer { pending = nil }
            do {
                try await projects.update(task, in: projectId, change: change)
                if change == .close {
                    screen.finishedTasks += 1
                }
            } catch {
                projects.report(error)
            }
        }
    }
}
