import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct RoomTaskSelection: Identifiable {
    let id: String
}

struct RoomTasks: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    @State private var composing = false
    @State private var opened: RoomTaskSelection?
    @ScaledMetric(relativeTo: .callout) private var plus: CGFloat = 12
    @ScaledMetric(relativeTo: .subheadline) private var chevron: CGFloat = 8

    var body: some View {
        let rooms = model.rooms
        let state = rooms.view(roomId)
        Group {
            if let snapshot = rooms.snapshots[roomId] {
                list(snapshot, state: state)
            } else if let failure = state.readFailure, !state.loading {
                RoomRecovery(message: failure) { Task { await rooms.read(roomId) } }
            } else {
                RoomSkeleton()
            }
        }
        .showsRoomPart(.tasks, of: state)
        .sheet(isPresented: $composing) { RoomTaskComposer(roomId: roomId) }
        .sheet(item: $opened) { RoomTaskSheet(roomId: roomId, taskId: $0.id) }
        .sensoryFeedback(.success, trigger: model.roomScreen(roomId).finishedTasks)
    }

    private func list(_ snapshot: RoomSnapshot, state: RoomViewState) -> some View {
        let scoped = snapshot.tasks.filter { task in state.repositoryFilter.map { task.repositoryIds.contains($0) } ?? true }
        let open = scoped.filter { !$0.closed }
        return List {
            header(snapshot.repositories, state: state)
                .cardRow(top: 10, bottom: 6)
            if scoped.isEmpty {
                EmptyState(title: "No tasks yet", message: "Put one up. People and agents can pick it up.") {
                    ProgressRing(value: 0, size: 44, lineWidth: 5)
                } actions: {
                    EmptyView()
                }
                .frame(minHeight: 240)
                .cardRow()
            }
            lane("Up for grabs", tasks: open.filter { $0.assignedTo == nil })
            lane("In progress", tasks: open.filter { $0.assignedTo != nil })
            done(scoped.filter(\.closed), state: state)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .readableWidth()
        .refreshable { await model.rooms.read(roomId) }
        .animation(theme.motion.spring, value: scoped.map { "\($0.id):\($0.version)" })
        .animation(theme.motion.spring, value: state.showsDone)
    }

    private func header(_ repositories: [RoomRepository], state: RoomViewState) -> some View {
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
            .accessibilityIdentifier("rooms.tasks.new")
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
                .accessibilityIdentifier("rooms.tasks.filter")
            }
        }
        .padding(.leading, 14).padding(.trailing, 6)
        .raised(Radius.xl)
    }

    @ViewBuilder
    private func lane(_ title: LocalizedStringKey, tasks: [RoomTask]) -> some View {
        if !tasks.isEmpty {
            HStack(spacing: 6) {
                Text(title).appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
                Text(tasks.count, format: .number).appTextStyle(.footnote).monospacedDigit().foregroundStyle(theme.colors.inkFaint)
                    .contentTransition(.numericText())
            }
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isHeader)
            .cardRow(top: 16, bottom: 2)
            ForEach(tasks) { task in
                RoomTaskRow(roomId: roomId, task: task) { opened = RoomTaskSelection(id: task.id) }
            }
        }
    }

    @ViewBuilder
    private func done(_ tasks: [RoomTask], state: RoomViewState) -> some View {
        if !tasks.isEmpty {
            Button { state.showsDone.toggle() } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: chevron, weight: .bold))
                        .rotationEffect(.degrees(state.showsDone ? 90 : 0))
                    Text("Done").appTextStyle(.subhead)
                    Text(tasks.count, format: .number).appTextStyle(.footnote).monospacedDigit()
                    Spacer(minLength: 0)
                }
                .foregroundStyle(theme.colors.inkMuted)
                .frame(minHeight: 32)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(state.showsDone ? Text("Shown") : Text("Hidden"))
            .accessibilityIdentifier("rooms.tasks.done.toggle")
            .cardRow(top: 12, bottom: 2)
            if state.showsDone {
                ForEach(tasks) { task in
                    RoomTaskRow(roomId: roomId, task: task) { opened = RoomTaskSelection(id: task.id) }
                }
            }
        }
    }
}

struct RoomTaskRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    let task: RoomTask
    let open: () -> Void
    @State private var pending: RoomTaskChange?
    @ScaledMetric(relativeTo: .body) private var checkSize: CGFloat = 20

    private var token: String {
        AccessibilityIdentifier.token(task.id)
    }

    private var linked: Bool {
        task.issue != nil
    }

    private var mine: Bool {
        task.assignedTo != nil && task.assignedTo == model.rooms.userId
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            check
            Button(action: open) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(task.title)
                        .appTextStyle(.body)
                        .fontWeight(.medium)
                        .foregroundStyle(task.closed ? theme.colors.inkMuted : theme.colors.ink)
                        .strikethrough(task.closed, color: theme.colors.inkFaint)
                        .multilineTextAlignment(.leading)
                        .lineLimit(3)
                    RoomTaskChips(roomId: roomId, task: task)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("rooms.task.\(token)")
            trailing
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .wash(trigger: task.version)
        .raised(Radius.lg)
        .overlay(alignment: .leading) {
            if mine, !task.closed {
                Capsule().fill(theme.colors.accent).frame(width: 3).padding(.vertical, 12).padding(.leading, -1)
            }
        }
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
            .accessibilityIdentifier("rooms.task.\(token).complete")
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
            .accessibilityIdentifier("rooms.task.\(token).claim")
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

    private func change(_ change: RoomTaskChange) {
        guard pending == nil else { return }
        pending = change
        let rooms = model.rooms
        let screen = model.roomScreen(roomId)
        let roomId = roomId
        let task = task
        Task {
            defer { pending = nil }
            do {
                try await rooms.update(task, in: roomId, change: change)
                if change == .close {
                    screen.finishedTasks += 1
                }
            } catch {
                rooms.report(error)
            }
        }
    }
}

struct RoomTaskChips: View {
    @Environment(AppModel.self) private var model
    let roomId: String
    let task: RoomTask

    var body: some View {
        let repositories = (model.rooms.snapshots[roomId]?.repositories ?? []).filter { task.repositoryIds.contains($0.id) }
        let terminal = task.terminalId.flatMap { model.terminals.session($0) }
        let note = task.closed ? task.note.flatMap { $0.isEmpty ? nil : $0 } : nil
        if !repositories.isEmpty || task.issue != nil || terminal != nil || note != nil {
            FlowLayout(spacing: 5) {
                ForEach(repositories) { Tag(text: $0.name, symbol: "shippingbox") }
                if let issue = task.issue {
                    Tag(text: "#\(issue.number)", symbol: "smallcircle.filled.circle")
                }
                if let terminal {
                    Tag(text: terminal.name, symbol: "apple.terminal", tone: terminal.isWorking ? .accent : .neutral)
                }
                if let note {
                    Tag(text: note, symbol: "text.quote", tone: .ready)
                }
            }
        }
    }
}
