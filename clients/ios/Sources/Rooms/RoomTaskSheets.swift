import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomTaskSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let roomId: String
    let taskId: String
    @State private var busy: RoomTaskChange?
    @State private var failure: String?
    @State private var saved = 0

    var body: some View {
        let task = model.rooms.snapshots[roomId]?.tasks.first { $0.id == taskId }
        NavigationStack {
            Group {
                if let task {
                    details(task)
                } else {
                    ContentUnavailableView("This task is not here anymore", systemImage: "checklist")
                }
            }
            .navigationTitle("Task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .accessibilityIdentifier("rooms.task.sheet.done")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
        .sensoryFeedback(.success, trigger: saved)
    }

    private func details(_ task: RoomTask) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                status(task)
                Text(task.title)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                RoomTaskChips(roomId: roomId, task: task)
                if !task.description.isEmpty {
                    Text(RoomMessageText.formatted(task.description))
                        .appTextStyle(.body)
                        .lineSpacing(2)
                        .foregroundStyle(theme.colors.inkMuted)
                        .tint(theme.colors.accentStrong)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
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

    private func status(_ task: RoomTask) -> some View {
        HStack(spacing: 8) {
            if task.closed {
                Tag(text: String(localized: "Done"), symbol: "checkmark", tone: .ready)
            } else if task.assignedTo == nil {
                Tag(text: String(localized: "Up for grabs"), symbol: "circle.dashed", tone: .accent)
            } else {
                Tag(text: String(localized: "In progress"), symbol: "circle.lefthalf.filled")
            }
            if let person = task.assignedName {
                PersonAvatar(name: person, key: task.assignedTo, size: 22, isSelf: task.assignedTo == model.rooms.userId)
                Text(person)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .lineLimit(1)
            }
        }
    }

    private func issueNote(_ issue: RoomIssue) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let url = RoomTaskChips.address(issue) {
                Link(destination: url) {
                    Label("Open issue #\(issue.number)", systemImage: "arrow.up.right")
                }
                .buttonStyle(.kodosi(.secondary, size: .small))
                .accessibilityIdentifier("rooms.task.issue")
            }
            Text("Its repository is in charge of this task. Change it there, or from Kodosi on a computer.")
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func controls(_ task: RoomTask) -> some View {
        let state = model.rooms.view(roomId)
        let note = Binding(get: { state.notes[task.id] ?? task.note ?? "" }, set: { state.notes[task.id] = $0 })
        let edited = (state.notes[task.id].map { $0 != (task.note ?? "") }) ?? false
        return VStack(alignment: .leading, spacing: 12) {
            TextField("Result or pull request link", text: note, axis: .vertical)
                .lineLimit(1 ... 4)
                .textFieldStyle(WellTextFieldStyle(radius: Radius.lg))
                .accessibilityIdentifier("rooms.task.note")
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
        _ title: LocalizedStringKey, _ change: RoomTaskChange, kind: KodosiButtonStyle.Kind, task: RoomTask, note: String? = nil
    ) -> some View {
        Button {
            perform(change, on: task, note: note)
        } label: {
            Text(title).shimmer(busy == change)
        }
        .buttonStyle(.kodosi(kind, size: .regular))
        .disabled(busy != nil)
        .accessibilityIdentifier("rooms.task.sheet.\(change.rawValue)")
    }

    private func perform(_ change: RoomTaskChange, on task: RoomTask, note: String?) {
        guard busy == nil else { return }
        busy = change
        failure = nil
        let rooms = model.rooms
        let roomId = roomId
        Task {
            defer { busy = nil }
            do {
                try await rooms.update(task, in: roomId, change: change, note: note)
                rooms.view(roomId).notes[task.id] = nil
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

struct RoomTaskComposer: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let roomId: String
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
        let linkable = model.rooms.snapshots[roomId]?.repositories ?? []
        NavigationStack {
            List {
                Section {
                    TextField("Add a task", text: $title, axis: .vertical)
                        .lineLimit(1 ... 3)
                        .appTextStyle(.body)
                        .focused($focused)
                        .accessibilityIdentifier("rooms.task.title")
                    TextField("Context for whoever picks it up", text: $details, axis: .vertical)
                        .lineLimit(3 ... 8)
                        .appTextStyle(.body)
                        .accessibilityIdentifier("rooms.task.description")
                }
                .listRowBackground(theme.colors.raised)
                if !linkable.isEmpty {
                    Section("Repositories") {
                        RoomFlowLayout(spacing: 6) {
                            ForEach(linkable) { repository in
                                repositoryToggle(repository)
                            }
                        }
                        .padding(.vertical, 4)
                    }
                    .listRowBackground(theme.colors.raised)
                }
                if let failure {
                    Section {
                        ErrorNote(message: failure)
                            .listRowInsets(EdgeInsets())
                            .listRowBackground(Color.clear)
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(theme.colors.surface)
            .navigationTitle("New task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .accessibilityIdentifier("rooms.task.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(busy ? "Adding…" : "Add", action: create)
                        .disabled(!valid || busy)
                        .accessibilityIdentifier("rooms.task.create")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
        .interactiveDismissDisabled(busy)
        .sensoryFeedback(.success, trigger: added)
        .onAppear { focused = true }
    }

    private func repositoryToggle(_ repository: RoomRepository) -> some View {
        let on = repositories.contains(repository.id)
        return Button {
            if on {
                repositories.remove(repository.id)
            } else {
                repositories.insert(repository.id)
            }
        } label: {
            Tag(text: repository.name, symbol: on ? "checkmark" : "shippingbox", tone: on ? .accent : .neutral)
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityAddTraits(on ? .isSelected : [])
        .accessibilityIdentifier("rooms.task.repository.\(AccessibilityIdentifier.token(repository.id))")
    }

    private func create() {
        guard valid, !busy else { return }
        busy = true
        failure = nil
        let rooms = model.rooms
        let roomId = roomId
        Task {
            defer { busy = false }
            do {
                try await rooms.createTask(in: roomId, title: title, description: details, repositoryIds: repositories)
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
