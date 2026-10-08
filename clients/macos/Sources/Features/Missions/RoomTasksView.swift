import SwiftUI

struct RoomTasksView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @Bindable var state: RoomViewState
    @FocusState private var adding: Bool

    private var tasks: [RoomTask] {
        deps.rooms[roomId]?.tasks ?? []
    }

    private var repositories: [RoomRepository] {
        deps.rooms[roomId]?.repositories ?? []
    }

    var body: some View {
        let scoped = tasks.filter { state.taskFilter.isEmpty || $0.repositoryIds.contains(state.taskFilter) }
        let open = scoped.filter { !$0.closed }
        let grabs = open.filter { $0.assignedTo == nil }
        let going = open.filter { $0.assignedTo != nil }
        let done = scoped.filter(\.closed)
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                composer
                if scoped.isEmpty {
                    EmptyState(title: "No tasks yet", message: "Write one above. People and agents can pick it up.") {
                        ProgressRing(value: 0, size: 44, lineWidth: 5)
                    } actions: { EmptyView() }
                        .frame(height: 260)
                }
                lane("Up for grabs", tasks: grabs)
                lane("In progress", tasks: going)
                if !done.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Button {
                            withAnimation(theme.motion.spring) { state.showsCompleted.toggle() }
                        } label: {
                            HStack(spacing: 6) {
                                Image(systemName: "chevron.right").font(.system(size: 8, weight: .bold))
                                    .rotationEffect(.degrees(state.showsCompleted ? 90 : 0))
                                Text("Done").appTextStyle(.subhead)
                                Text(done.count, format: .number).appTextStyle(.footnote).monospacedDigit().contentTransition(.numericText())
                            }
                            .foregroundStyle(theme.colors.inkMuted).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain).accessibilityIdentifier("room.task.done.toggle")
                        if state.showsCompleted {
                            ForEach(done) { RoomTaskCard(roomId: roomId, task: $0, state: state).transition(AnyTransition.rise) }
                        }
                    }
                }
            }
            .padding(.horizontal, 18).padding(.top, 6).padding(.bottom, 24)
            .frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        }
        .animation(theme.motion.spring, value: tasks.map { "\($0.id):\($0.closed):\($0.assignedTo ?? "")" })
    }

    @ViewBuilder
    private func lane(_ title: LocalizedStringKey, tasks: [RoomTask]) -> some View {
        if !tasks.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 6) {
                    Text(title).appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
                    Text(tasks.count, format: .number).appTextStyle(.footnote).monospacedDigit().foregroundStyle(theme.colors.inkFaint)
                        .contentTransition(.numericText())
                }
                ForEach(tasks) { RoomTaskCard(roomId: roomId, task: $0, state: state).transition(AnyTransition.rise) }
            }
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "plus").font(.system(size: 12, weight: .bold))
                    .foregroundStyle(adding ? theme.colors.accentStrong : theme.colors.inkFaint)
                TextField("Add a task", text: $state.taskTitle)
                    .textFieldStyle(.plain).appTextStyle(.callout).focused($adding)
                    .onSubmit(create).accessibilityIdentifier("room.task.title")
                if !repositories.isEmpty {
                    Menu {
                        Button("All repositories") { state.taskFilter = "" }
                        ForEach(repositories) { repository in Button(repository.name) { state.taskFilter = repository.id } }
                    } label: {
                        Label(repositories.first { $0.id == state.taskFilter }?.name ?? String(localized: "All"),
                              systemImage: "line.3.horizontal.decrease")
                    }
                    .menuStyle(.button).buttonStyle(.kodosi(.ghost, size: .small)).fixedSize().help("Show tasks for one repository")
                }
                if !state.taskTitle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Button("Add", action: create).buttonStyle(.kodosi(.primary, size: .small)).disabled(state.busy)
                        .accessibilityIdentifier("room.task.create").transition(AnyTransition.pop)
                }
            }
            .padding(.leading, 14).padding(.trailing, 6).frame(height: 40)
            if adding || !state.taskDescription.isEmpty || !state.taskRepositories.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    TextField("Context for whoever picks it up", text: $state.taskDescription, axis: .vertical)
                        .textFieldStyle(.plain).appTextStyle(.body).lineLimit(2 ... 6)
                        .accessibilityIdentifier("room.task.description")
                    if !repositories.isEmpty {
                        FlowRow(spacing: 6) {
                            ForEach(repositories) { repository in
                                let on = state.taskRepositories.contains(repository.id)
                                Button {
                                    if on {
                                        state.taskRepositories.remove(repository.id)
                                    } else {
                                        state.taskRepositories.insert(repository.id)
                                    }
                                } label: { Tag(text: repository.name, symbol: on ? "checkmark" : "shippingbox", tone: on ? .accent : .neutral) }
                                    .buttonStyle(PressScaleStyle())
                            }
                        }
                    }
                }
                .padding(.horizontal, 14).padding(.bottom, 12)
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .raised(Radius.xl, fill: theme.colors.raised, elevation: adding ? .lifted : .resting)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(adding ? 0.5 : 0), lineWidth: 1.5)
        }
        .animation(theme.motion.spring, value: adding)
        .animation(theme.motion.snappy, value: state.taskTitle.isEmpty)
    }

    private func create() {
        let title = state.taskTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, !state.busy else { return }
        let revision = state.taskRevision
        state.busy = true; state.failure = nil
        Task { @MainActor in
            defer { state.busy = false }
            do {
                try await deps.roomAction(roomId, [
                    "type": .string("createTask"), "title": .string(title),
                    "description": .string(state.taskDescription),
                    "repositoryIds": .array(state.taskRepositories.sorted().map(JSONValue.string)),
                ])
                if revision == state.taskRevision {
                    state.taskTitle = ""; state.taskDescription = ""; state.taskRepositories.removeAll()
                }
            } catch { state.failure = error.localizedDescription }
        }
    }
}

private struct RoomTaskCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    let task: RoomTask
    @Bindable var state: RoomViewState
    @State private var hovered = false

    private var expanded: Bool {
        state.expandedTask == task.id
    }

    private var mine: Bool {
        task.assignedTo != nil && task.assignedTo == deps.userId
    }

    private var repositories: [RoomRepository] {
        (deps.rooms[roomId]?.repositories ?? []).filter { task.repositoryIds.contains($0.id) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                Button { update(task.closed ? "reopen" : "close") } label: {
                    Image(systemName: task.closed ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 19, weight: .regular))
                        .foregroundStyle(task.closed ? theme.colors.ready : hovered ? theme.colors.accent : theme.colors.inkFaint)
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(PressScaleStyle(scale: 0.85)).disabled(state.busy)
                .help(task.closed ? "Reopen" : "Complete")
                .accessibilityLabel(task.closed ? Text("Reopen \(task.title)") : Text("Complete \(task.title)"))
                .accessibilityIdentifier("room.task.\(AccessibilityIdentifier.token(task.id)).complete")
                Button {
                    withAnimation(theme.motion.spring) { state.expandedTask = expanded ? nil : task.id }
                } label: {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(task.title).appTextStyle(.body).fontWeight(.medium).lineLimit(expanded ? nil : 2)
                            .foregroundStyle(task.closed ? theme.colors.inkMuted : theme.colors.ink)
                            .strikethrough(task.closed, color: theme.colors.inkFaint)
                            .multilineTextAlignment(.leading)
                        chips
                    }
                    .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                }
                .buttonStyle(.plain).accessibilityIdentifier("room.task.\(AccessibilityIdentifier.token(task.id))")
                trailing
            }
            .padding(.horizontal, 14).padding(.vertical, 12)
            if expanded {
                detail.padding(.leading, 45).padding(.trailing, 14).padding(.bottom, 14)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .wash(trigger: task.version)
        .raised(Radius.lg, fill: hovered && !expanded ? theme.colors.lifted : theme.colors.raised, elevation: expanded ? .lifted : .resting)
        .overlay(alignment: .leading) {
            if mine, !task.closed {
                Capsule().fill(theme.colors.accent).frame(width: 3).padding(.vertical, 12).padding(.leading, -1)
            }
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
    }

    @ViewBuilder
    private var chips: some View {
        let terminal = task.terminalId.flatMap(deps.session)
        if !repositories.isEmpty || task.issue != nil || terminal != nil || (task.closed && task.note?.isEmpty == false) {
            FlowRow(spacing: 5) {
                ForEach(repositories) { Tag(text: $0.name, symbol: "shippingbox") }
                if let issue = task.issue {
                    Tag(text: "#\(issue.number)", symbol: "smallcircle.filled.circle")
                }
                if let terminal {
                    Tag(text: terminal.name, symbol: "apple.terminal", tone: terminal.isWorking ? .accent : .neutral)
                }
                if task.closed, let note = task.note, !note.isEmpty {
                    Tag(text: note, symbol: "text.quote", tone: .ready)
                }
            }
        }
    }

    @ViewBuilder
    private var trailing: some View {
        if let person = task.assignedName {
            PersonAvatar(name: person, key: task.assignedTo, size: 24, isSelf: mine).help(person)
        } else if !task.closed {
            Button("Pick up") { update("claim") }
                .buttonStyle(.kodosi(.tinted, size: .small)).disabled(state.busy)
                .opacity(hovered || expanded ? 1 : 0)
                .accessibilityLabel(Text("Pick up \(task.title)"))
                .accessibilityIdentifier("room.task.\(AccessibilityIdentifier.token(task.id)).claim")
        }
    }

    private var detail: some View {
        VStack(alignment: .leading, spacing: 12) {
            if !task.description.isEmpty {
                Text(.init(task.description)).appTextStyle(.body).lineSpacing(2.5).foregroundStyle(theme.colors.inkMuted)
                    .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }
            if let issue = task.issue, let url = URL(string: issue.url), ["https", "http"].contains(url.scheme ?? "") {
                Link(destination: url) { Label("Open issue #\(issue.number)", systemImage: "arrow.up.right") }
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.accentStrong)
            }
            TextField("Result or pull request link", text: Binding(
                get: { state.completionNotes[task.id] ?? task.note ?? "" },
                set: { state.completionNotes[task.id] = $0 }
            ), axis: .vertical)
                .lineLimit(1 ... 4).accessibilityIdentifier("room.task.note")
            HStack(spacing: 8) {
                if !task.closed, task.assignedTo != nil {
                    Button("Release") { update("release") }.buttonStyle(.kodosi(.ghost, size: .small))
                }
                Spacer()
                Button(task.closed ? "Save note" : "Complete") { update("close", note: state.completionNotes[task.id]) }
                    .buttonStyle(.kodosi(.primary, size: .small))
            }
            .disabled(state.busy)
        }
    }

    private func update(_ change: String, note: String? = nil) {
        var action: [String: JSONValue] = ["type": .string("updateTask"), "taskId": .string(task.id), "change": .string(change)]
        if let note, !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            action["note"] = .string(note)
        }
        state.busy = true; state.failure = nil
        Task { @MainActor in
            defer { state.busy = false }
            do {
                try await deps.roomAction(roomId, action)
            } catch { state.failure = error.localizedDescription }
        }
    }
}
