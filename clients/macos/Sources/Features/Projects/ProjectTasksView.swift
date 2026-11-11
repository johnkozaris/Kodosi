import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectTasksView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let projectId: String
    let state: ProjectViewState
    @Bindable var workspace: ProjectWorkspace
    @FocusState private var adding: Bool

    private var tasks: [ProjectTask] {
        deps.projects.snapshots[projectId]?.tasks ?? []
    }

    private var repositories: [ProjectRepository] {
        deps.projects.snapshots[projectId]?.repositories ?? []
    }

    var body: some View {
        let lanes = ProjectTaskLanes(tasks, repository: state.repositoryFilter)
        let done = lanes.done
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                composer
                if lanes.tasks.isEmpty {
                    EmptyState(title: "No tasks yet", message: "Write one above. People and agents can pick it up.") {
                        ProgressRing(value: 0, size: 44, lineWidth: 5)
                    } actions: { EmptyView() }
                        .frame(height: 260)
                }
                lane(.upForGrabs, tasks: lanes.upForGrabs)
                lane(.inProgress, tasks: lanes.inProgress)
                if !done.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        ProjectDoneTasksKey(state: state, count: done.count, identifier: "project.task.done.toggle")
                        if state.showsDone {
                            ForEach(done) { ProjectTaskCard(projectId: projectId, task: $0, state: state, workspace: workspace).transition(AnyTransition.rise) }
                        }
                    }
                }
            }
            .padding(.horizontal, 18).padding(.top, 6).padding(.bottom, 24)
            .frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        }
        .animation(theme.motion.spring, value: tasks.map { "\($0.id):\($0.closed):\($0.assignedTo ?? "")" })
        .showsProjectPart(.tasks, of: state)
    }

    @ViewBuilder
    private func lane(_ lane: ProjectTaskLaneTitle.Lane, tasks: [ProjectTask]) -> some View {
        if !tasks.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                ProjectTaskLaneTitle(lane, count: tasks.count)
                ForEach(tasks) { ProjectTaskCard(projectId: projectId, task: $0, state: state, workspace: workspace).transition(AnyTransition.rise) }
            }
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "plus").font(.system(size: 12, weight: .bold))
                    .foregroundStyle(adding ? theme.colors.accentStrong : theme.colors.inkFaint)
                TextField("Add a task", text: $workspace.taskTitle)
                    .textFieldStyle(.plain).appTextStyle(.callout).focused($adding)
                    .onSubmit(create).accessibilityIdentifier("project.task.title")
                if !repositories.isEmpty {
                    Menu {
                        Button("All repositories") { state.repositoryFilter = nil }
                        ForEach(repositories) { repository in Button(repository.name) { state.repositoryFilter = repository.id } }
                    } label: {
                        Label(repositories.first { $0.id == state.repositoryFilter }?.name ?? String(localized: "All"),
                              systemImage: "line.3.horizontal.decrease")
                    }
                    .menuStyle(.button).buttonStyle(.kodosi(.ghost, size: .small)).fixedSize().help("Show tasks for one repository")
                }
                if !workspace.taskTitle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Button("Add", action: create).buttonStyle(.kodosi(.primary, size: .small)).disabled(workspace.busy)
                        .accessibilityIdentifier("project.task.create").transition(AnyTransition.pop)
                } else {
                    IconButton(title: "Add issues", symbol: "smallcircle.filled.circle", identifier: "project.task.issues", size: 28,
                               open: workspace.showsIssuePicker)
                    {
                        workspace.showsIssuePicker = true
                    }
                    .kodosiPopover(isPresented: $workspace.showsIssuePicker, alignment: .trailing) {
                        IssuePicker(projectId: projectId).padding(12).frame(width: 420)
                    }
                    .transition(AnyTransition.pop)
                }
            }
            .padding(.leading, 14).padding(.trailing, 6).frame(height: 40)
            if adding || !workspace.taskDescription.isEmpty || !workspace.taskRepositories.isEmpty || workspace.taskIssueRepository != nil {
                VStack(alignment: .leading, spacing: 8) {
                    TextField("Context for whoever picks it up", text: $workspace.taskDescription, axis: .vertical)
                        .textFieldStyle(.plain).appTextStyle(.body).lineLimit(2 ... 6)
                        .accessibilityIdentifier("project.task.description")
                    if !repositories.isEmpty {
                        FlowLayout(spacing: 6) {
                            ForEach(repositories) { repository in
                                let on = workspace.taskRepositories.contains(repository.id)
                                Button {
                                    if on {
                                        workspace.taskRepositories.remove(repository.id)
                                    } else {
                                        workspace.taskRepositories.insert(repository.id)
                                    }
                                } label: { Tag(text: repository.name, symbol: on ? "checkmark" : "shippingbox", tone: on ? .accent : .neutral) }
                                    .buttonStyle(PressScaleStyle())
                            }
                            issueChoice
                        }
                    }
                }
                .padding(.horizontal, 14).padding(.bottom, 12)
                .transition(AnyTransition.unfold)
            }
        }
        .raised(Radius.xl, fill: theme.colors.raised, elevation: adding ? .lifted : .resting)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(adding ? 0.5 : 0), lineWidth: 1.5)
        }
        .animation(theme.motion.spring, value: adding)
        .animation(theme.motion.snappy, value: workspace.taskTitle.isEmpty)
    }

    private var issueChoice: some View {
        let chosen = repositories.first { $0.id == workspace.taskIssueRepository }
        return Menu {
            if chosen != nil {
                Button("No issue") { workspace.taskIssueRepository = nil }
                Divider()
            }
            ForEach(repositories) { repository in
                Button(repository.name) { workspace.taskIssueRepository = repository.id }
            }
        } label: {
            Tag(text: chosen.map { String(localized: "Issue in \($0.name)") } ?? String(localized: "Open as an issue"),
                symbol: "smallcircle.filled.circle", tone: chosen == nil ? .neutral : .accent)
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
        .accessibilityIdentifier("project.task.issue")
    }

    private func create() {
        let title = workspace.taskTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, !workspace.busy else { return }
        let revision = workspace.taskRevision
        workspace.busy = true; state.failure = nil
        Task { @MainActor in
            defer { workspace.busy = false }
            do {
                try await deps.projects.createTask(
                    in: projectId, title: title, description: workspace.taskDescription, repositoryIds: workspace.taskRepositories,
                    issueRepositoryId: workspace.taskIssueRepository,
                )
                if revision == workspace.taskRevision {
                    workspace.taskTitle = ""; workspace.taskDescription = ""; workspace.taskRepositories.removeAll()
                    workspace.taskIssueRepository = nil
                }
            } catch { state.failure = error.localizedDescription }
        }
    }
}

private struct ProjectTaskCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let projectId: String
    let task: ProjectTask
    let state: ProjectViewState
    let workspace: ProjectWorkspace
    @State private var hovered = false

    private var expanded: Bool {
        workspace.expandedTask == task.id
    }

    private var mine: Bool {
        task.assignedTo != nil && task.assignedTo == deps.account.userId
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                Button { update(task.closed ? .reopen : .close) } label: {
                    Image(systemName: task.closed ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 19, weight: .regular))
                        .foregroundStyle(task.closed ? theme.colors.ready : hovered ? theme.colors.accent : theme.colors.inkFaint)
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(PressScaleStyle()).disabled(workspace.busy)
                .help(task.closed ? "Reopen" : "Complete")
                .accessibilityLabel(task.closed ? Text("Reopen \(task.title)") : Text("Complete \(task.title)"))
                .accessibilityIdentifier("project.task.\(AccessibilityIdentifier.token(task.id)).complete")
                Button {
                    withAnimation(theme.motion.spring) { workspace.expandedTask = expanded ? nil : task.id }
                } label: {
                    VStack(alignment: .leading, spacing: 6) {
                        ProjectTaskTitle(task: task, lines: expanded ? nil : 2)
                        ProjectTaskChips(
                            task: task, repositories: deps.projects.snapshots[projectId]?.repositories ?? [],
                            terminal: task.terminalId.flatMap(deps.session),
                        )
                    }
                    .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                }
                .buttonStyle(.plain).accessibilityIdentifier("project.task.\(AccessibilityIdentifier.token(task.id))")
                trailing
            }
            .padding(.horizontal, 14).padding(.vertical, 12)
            if expanded {
                detail.padding(.leading, 45).padding(.trailing, 14).padding(.bottom, 14)
                    .transition(AnyTransition.unfold)
            }
        }
        .wash(trigger: task.version)
        .raised(Radius.lg, fill: hovered && !expanded ? theme.colors.lifted : theme.colors.raised, elevation: expanded ? .lifted : .resting)
        .projectTaskMarker(mine && !task.closed)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
    }

    @ViewBuilder
    private var trailing: some View {
        if let person = task.assignedName {
            PersonAvatar(name: person, key: task.assignedTo, size: 24, isSelf: mine).help(person)
        } else if !task.closed {
            Button("Pick up") { update(.claim) }
                .buttonStyle(.kodosi(.tinted, size: .small)).disabled(workspace.busy)
                .opacity(hovered || expanded ? 1 : 0)
                .accessibilityLabel(Text("Pick up \(task.title)"))
                .accessibilityIdentifier("project.task.\(AccessibilityIdentifier.token(task.id)).claim")
        }
    }

    private var detail: some View {
        VStack(alignment: .leading, spacing: 12) {
            if !task.description.isEmpty {
                ProjectTaskDescription(text: task.description)
            }
            if let issue = task.issue, let url = issue.address {
                Link(destination: url) { Label("Open issue #\(issue.number)", systemImage: "arrow.up.right") }
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.accentStrong)
            }
            TextField("Result or pull request link", text: Binding(
                get: { state.notes[task.id] ?? task.note ?? "" },
                set: { state.notes[task.id] = $0 },
            ), axis: .vertical)
                .lineLimit(1 ... 4).accessibilityIdentifier("project.task.note")
            HStack(spacing: 8) {
                if !task.closed, task.assignedTo != nil {
                    Button("Release") { update(.release) }.buttonStyle(.kodosi(.ghost, size: .small))
                }
                Spacer()
                Button(task.closed ? "Save note" : "Complete") { update(.close, note: state.notes[task.id]) }
                    .buttonStyle(.kodosi(.primary, size: .small))
            }
            .disabled(workspace.busy)
        }
    }

    private func update(_ change: ProjectTaskChange, note: String? = nil) {
        workspace.busy = true; state.failure = nil
        Task { @MainActor in
            defer { workspace.busy = false }
            do {
                try await deps.projects.update(task, in: projectId, change: change, note: note)
            } catch { state.failure = error.localizedDescription }
        }
    }
}
