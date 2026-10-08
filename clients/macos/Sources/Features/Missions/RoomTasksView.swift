import SwiftUI

struct RoomTasksView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let roomId: String
    @Bindable var state: RoomViewState
    private var tasks: [RoomTask] {
        deps.rooms[roomId]?.tasks ?? []
    }

    private var repositories: [RoomRepository] {
        deps.rooms[roomId]?.repositories ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Toggle(isOn: $state.showsCompleted) { Label("Done", systemImage: "checkmark.circle") }
                    .toggleStyle(.button).buttonStyle(.plain)
                    .foregroundStyle(state.showsCompleted ? theme.colors.primary : theme.colors.mutedForeground)
                Text(tasks.filter(\.closed).count, format: .number).appTextStyle(.caption).contentTransition(.numericText())
                Spacer()
                if !repositories.isEmpty {
                    Picker("Repository", selection: $state.taskFilter) {
                        Text("All repositories").tag("")
                        ForEach(repositories) { Text($0.name).tag($0.id) }
                    }.labelsHidden().frame(maxWidth: 200)
                }
                Button { state.showsTaskForm.toggle() } label: { Label("New task", systemImage: "plus") }
                    .buttonStyle(SolidPrimaryButtonStyle()).accessibilityIdentifier("room.task.new")
                    .popover(isPresented: $state.showsTaskForm) { taskForm }
            }.padding(16)
            let visible = tasks
                .filter { ($0.closed == state.showsCompleted) && (state.taskFilter.isEmpty || $0.repositoryIds.contains(state.taskFilter)) }
            if visible.isEmpty {
                VStack(spacing: 16) {
                    Image(systemName: state.showsCompleted ? "checkmark.circle" : "checklist").font(.system(size: 44, weight: .ultraLight))
                        .foregroundStyle(theme.colors.tertiary)
                    if !state.showsCompleted {
                        Button("New task") { state.showsTaskForm = true }
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) { ForEach(visible) { taskRow($0).id($0.id) } }.scrollTargetLayout().padding(.horizontal, 16)
                }.scrollPosition(id: $state.taskReading).animation(reduceMotion ? nil : theme.motion.selection, value: visible.map(\.id))
            }
        }
    }

    private func taskRow(_ task: RoomTask) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .center, spacing: 12) {
                Button { update(task, task.closed ? "reopen" : "close") } label: {
                    Image(systemName: task.closed ? "checkmark.circle.fill" : "circle").font(.system(size: 20, weight: .light))
                        .foregroundStyle(task.closed ? theme.colors.tertiary : theme.colors.mutedForeground)
                }.buttonStyle(.plain).disabled(state.busy).help(task.closed ? "Reopen task" : "Complete task")
                    .accessibilityLabel(task.closed ? Text("Reopen \(task.title)") : Text("Complete \(task.title)"))
                    .accessibilityIdentifier("room.task.\(AccessibilityIdentifier.token(task.id)).complete")
                Button {
                    withAnimation(reduceMotion ? nil : .smooth(duration: 0.22)) { state.expandedTask = state.expandedTask == task.id ? nil : task.id }
                } label: {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(task.title).font(.system(size: 13, weight: .medium)).lineLimit(2).foregroundStyle(theme.colors.foreground)
                        HStack(spacing: 7) {
                            ForEach(repositories.filter { task.repositoryIds.contains($0.id) }) { repository in
                                Text(repository.name).font(.system(size: 10)).foregroundStyle(theme.colors.primary)
                            }
                            if let issue = task.issue {
                                Text("#\(issue.number)").font(.system(size: 10)).foregroundStyle(theme.colors.mutedForeground)
                            }
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                }.buttonStyle(.plain).accessibilityIdentifier("room.task.\(AccessibilityIdentifier.token(task.id))")
                if let person = task.assignedName {
                    RoomAvatar(name: person, size: 24).help(person)
                } else if !task.closed {
                    Button { update(task, "claim") } label: { Image(systemName: "person.badge.plus") }
                        .buttonStyle(.plain).help("Pick up task").accessibilityLabel(Text("Pick up \(task.title)"))
                }
                Image(systemName: state.expandedTask == task.id ? "chevron.up" : "chevron.down").font(.system(size: 9))
                    .foregroundStyle(theme.colors.mutedForeground)
                    .accessibilityHidden(true)
            }.padding(.vertical, 14)
            if state.expandedTask == task.id {
                VStack(alignment: .leading, spacing: 12) {
                    if !task.description.isEmpty {
                        Text(.init(task.description)).appTextStyle(.body).textSelection(.enabled)
                    }
                    if let issue = task.issue, let url = URL(string: issue.url), ["https", "http"].contains(url.scheme ?? "") {
                        Link(destination: url) { Label("Open issue", systemImage: "arrow.up.right") }.font(.system(size: 12))
                    }
                    TextField("Result or pull request link", text: Binding(
                        get: { state.completionNotes[task.id] ?? task.note ?? "" },
                        set: { state.completionNotes[task.id] = $0 }
                    ), axis: .vertical)
                        .lineLimit(1 ... 4).accessibilityIdentifier("room.task.note")
                    HStack {
                        if !task.closed {
                            Button(task.assignedTo == nil ? "Pick up" : "Release") { update(task, task.assignedTo == nil ? "claim" : "release") }
                        }
                        Spacer()
                        Button(task.closed ? "Save note" : "Complete") { update(task, "close", note: state.completionNotes[task.id]) }
                            .buttonStyle(SolidPrimaryButtonStyle())
                    }.disabled(state.busy)
                }.padding(.leading, 32).padding(.bottom, 16).transition(.opacity.combined(with: .move(edge: .top)))
            }
            Divider().opacity(0.55)
        }
    }

    private var taskForm: some View {
        VStack(alignment: .leading, spacing: 14) {
            TextField("Task title", text: $state.taskTitle).font(.system(size: 16, weight: .medium)).accessibilityIdentifier("room.task.title")
            TextField("Description", text: $state.taskDescription, axis: .vertical).lineLimit(3 ... 8)
                .accessibilityIdentifier("room.task.description")
            if !repositories.isEmpty {
                Menu {
                    ForEach(repositories) { repository in
                        Toggle(repository.name, isOn: Binding(get: { state.taskRepositories.contains(repository.id) }, set: { selected in
                            if selected {
                                state.taskRepositories.insert(repository.id)
                            } else {
                                state.taskRepositories.remove(repository.id)
                            }
                        }))
                    }
                } label: {
                    let title = state.taskRepositories.isEmpty
                        ? String(localized: "Repositories") : String(localized: "\(state.taskRepositories.count) repositories")
                    Label(title, systemImage: "point.3.connected.trianglepath.dotted")
                }
            }
            HStack {
                Button("Cancel") { state.showsTaskForm = false }
                Spacer()
                Button("Create task", action: create).buttonStyle(SolidPrimaryButtonStyle())
                    .disabled(state.busy || state.taskTitle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityIdentifier("room.task.create")
            }
        }.padding(20).frame(width: 360)
    }

    private func create() {
        let revision = state.taskRevision
        let action: [String: JSONValue] = [
            "type": .string("createTask"), "title": .string(state.taskTitle),
            "description": .string(state.taskDescription),
            "repositoryIds": .array(state.taskRepositories.sorted().map(JSONValue.string)),
        ]
        perform(action) {
            if revision == state.taskRevision {
                state.taskTitle = ""; state.taskDescription = ""; state.taskRepositories.removeAll(); state.showsTaskForm = false
            }
        }
    }

    private func update(_ task: RoomTask, _ change: String, note: String? = nil) {
        var action: [String: JSONValue] = ["type": .string("updateTask"), "taskId": .string(task.id), "change": .string(change)]
        if let note, !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            action["note"] = .string(note)
        }
        perform(action)
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
