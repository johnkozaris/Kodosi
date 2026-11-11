import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectTaskSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let projectId: String
    let taskId: String
    @State private var busy: ProjectTaskChange?
    @State private var failure: String?
    @State private var saved = 0

    var body: some View {
        let task = model.projects.snapshots[projectId]?.tasks.first { $0.id == taskId }
        NavigationStack {
            Group {
                if let task {
                    details(task)
                } else {
                    EmptyState(title: "This task is not here anymore") {
                        IconTile(symbol: "checklist", tint: TileTint.graphite, size: 52)
                    } actions: {
                        EmptyView()
                    }
                }
            }
            .background(theme.colors.surface)
            .navigationTitle("Task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Done", symbol: "xmark", identifier: "projects.task.sheet.done", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.medium, .large])
        .kodosiSheet()
        .sensoryFeedback(.success, trigger: saved)
    }

    private func details(_ task: ProjectTask) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                status(task)
                Text(task.title)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                ProjectTaskChips(
                    task: task, repositories: model.projects.snapshots[projectId]?.repositories ?? [],
                    terminal: task.terminalId.flatMap { model.terminals.session($0) },
                )
                if !task.description.isEmpty {
                    ProjectTaskDescription(text: task.description)
                }
                if let issue = task.issue {
                    issueNote(issue)
                } else {
                    controls(task)
                }
                if let failure {
                    ErrorNote(message: failure).transition(AnyTransition.rise)
                }
            }
            .padding(20)
            .frame(maxWidth: 640, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .animation(theme.motion.spring, value: failure)
    }

    private func status(_ task: ProjectTask) -> some View {
        HStack(spacing: 8) {
            if task.closed {
                Tag(text: String(localized: "Done"), symbol: "checkmark", tone: .ready)
            } else if task.assignedTo == nil {
                Tag(text: String(localized: "Up for grabs"), symbol: "circle.dashed", tone: .accent)
            } else {
                Tag(text: String(localized: "In progress"), symbol: "circle.lefthalf.filled")
            }
            if let person = task.assignedName {
                PersonAvatar(name: person, key: task.assignedTo, size: 22, isSelf: task.assignedTo == model.projects.userId)
                Text(person)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .lineLimit(1)
            }
        }
    }

    private func issueNote(_ issue: ProjectIssue) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let url = issue.address {
                Link(destination: url) {
                    Label("Open issue #\(issue.number)", systemImage: "arrow.up.right")
                }
                .buttonStyle(.kodosi(.secondary, size: .small))
                .accessibilityIdentifier("projects.task.issue")
            }
            Text("Its repository is in charge of this task. Change it there, or from Kodosi on a computer.")
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func controls(_ task: ProjectTask) -> some View {
        let state = model.projects.view(projectId)
        let note = Binding(get: { state.notes[task.id] ?? task.note ?? "" }, set: { state.notes[task.id] = $0 })
        let edited = (state.notes[task.id].map { $0 != (task.note ?? "") }) ?? false
        return VStack(alignment: .leading, spacing: 12) {
            TextField("Result or pull request link", text: note, axis: .vertical)
                .lineLimit(1 ... 4)
                .accessibilityIdentifier("projects.task.note")
            HStack(spacing: 8) {
                if task.closed {
                    action("Reopen", .reopen, kind: .ghost, task: task)
                } else if task.assignedTo == nil {
                    action("Pick up", .claim, kind: .tinted, task: task)
                } else {
                    action("Release", .release, kind: .ghost, task: task)
                }
                Spacer(minLength: 0)
                if task.closed {
                    action("Save note", .close, kind: .primary, task: task, note: state.notes[task.id])
                        .disabled(!edited || busy != nil)
                } else {
                    action("Complete", .close, kind: .primary, task: task, note: state.notes[task.id])
                }
            }
        }
    }

    private func action(
        _ title: LocalizedStringKey, _ change: ProjectTaskChange, kind: KodosiButtonStyle.Kind, task: ProjectTask, note: String? = nil,
    ) -> some View {
        Button {
            perform(change, on: task, note: note)
        } label: {
            Text(title).shimmer(busy == change)
        }
        .buttonStyle(.kodosi(kind, size: .regular))
        .disabled(busy != nil)
        .accessibilityIdentifier("projects.task.sheet.\(change.rawValue)")
    }

    private func perform(_ change: ProjectTaskChange, on task: ProjectTask, note: String?) {
        guard busy == nil else { return }
        busy = change
        failure = nil
        let projects = model.projects
        let projectId = projectId
        Task {
            defer { busy = nil }
            do {
                try await projects.update(task, in: projectId, change: change, note: note)
                projects.view(projectId).notes[task.id] = nil
                saved += 1
                if change == .close, !task.closed {
                    dismiss()
                }
            } catch is CancellationError {
                return
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}

struct ProjectTaskComposer: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let projectId: String
    @State private var title = ""
    @State private var details = ""
    @State private var repositories: Set<String> = []
    @State private var busy = false
    @State private var failure: String?
    @State private var added = 0
    @FocusState private var focused: Bool

    private var valid: Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        let linkable = model.projects.snapshots[projectId]?.repositories ?? []
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(spacing: 10) {
                            Image(systemName: "plus").font(.system(size: 12, weight: .bold))
                                .foregroundStyle(focused ? theme.colors.accentStrong : theme.colors.inkFaint)
                            TextField("Add a task", text: $title, axis: .vertical)
                                .textFieldStyle(.plain)
                                .tint(theme.colors.accent)
                                .lineLimit(1 ... 3)
                                .appTextStyle(.callout)
                                .focused($focused)
                                .accessibilityIdentifier("projects.task.title")
                        }
                        TextField("Context for whoever picks it up", text: $details, axis: .vertical)
                            .textFieldStyle(.plain)
                            .tint(theme.colors.accent)
                            .lineLimit(3 ... 8)
                            .appTextStyle(.body)
                            .accessibilityIdentifier("projects.task.description")
                        if !linkable.isEmpty {
                            FlowLayout(spacing: 6) {
                                ForEach(linkable) { repository in
                                    repositoryToggle(repository)
                                }
                            }
                        }
                    }
                    .padding(14)
                    .raised(Radius.xl, elevation: focused ? .lifted : .resting)
                    .overlay {
                        RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
                            .strokeBorder(theme.colors.accent.opacity(focused ? 0.5 : 0), lineWidth: 1.5)
                    }
                    if let failure {
                        ErrorNote(message: failure).transition(AnyTransition.rise)
                    }
                }
                .padding(20)
                .frame(maxWidth: 640)
                .frame(maxWidth: .infinity)
            }
            .background(theme.colors.surface)
            .navigationTitle("New task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .buttonStyle(.kodosi(.ghost, size: .small))
                        .fixedSize()
                        .accessibilityIdentifier("projects.task.cancel")
                }
                .sharedBackgroundVisibility(.hidden)
                ToolbarItem(placement: .confirmationAction) {
                    Button(action: create) {
                        Text(busy ? "Adding…" : "Add").shimmer(busy)
                    }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .fixedSize()
                    .disabled(!valid || busy)
                    .accessibilityIdentifier("projects.task.create")
                }
                .sharedBackgroundVisibility(.hidden)
            }
            .animation(theme.motion.spring, value: failure)
            .animation(theme.motion.hover, value: focused)
        }
        .presentationDetents([.medium, .large])
        .kodosiSheet()
        .interactiveDismissDisabled(busy)
        .sensoryFeedback(.success, trigger: added)
        .onAppear { focused = true }
    }

    private func repositoryToggle(_ repository: ProjectRepository) -> some View {
        let on = repositories.contains(repository.id)
        return Button {
            if on {
                repositories.remove(repository.id)
            } else {
                repositories.insert(repository.id)
            }
        } label: {
            Tag(text: repository.name, symbol: on ? "checkmark" : "shippingbox", tone: on ? .accent : .neutral)
                .touchTarget()
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityAddTraits(on ? .isSelected : [])
        .accessibilityIdentifier("projects.task.repository.\(AccessibilityIdentifier.token(repository.id))")
    }

    private func create() {
        guard valid, !busy else { return }
        busy = true
        failure = nil
        let projects = model.projects
        let projectId = projectId
        Task {
            defer { busy = false }
            do {
                try await projects.createTask(in: projectId, title: title, description: details, repositoryIds: repositories)
                added += 1
                dismiss()
            } catch is CancellationError {
                return
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}
