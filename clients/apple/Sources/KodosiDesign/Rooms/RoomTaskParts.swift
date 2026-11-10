import KodosiBridge
import SwiftUI

/// What a task is about: its repositories, its issue, the terminal that works on it, and its
/// result once it is done.
public struct RoomTaskChips: View {
    let task: RoomTask
    let repositories: [RoomRepository]
    let terminal: RuntimeSession?

    /// Shows the repositories of the room that the task names.
    public init(task: RoomTask, repositories: [RoomRepository], terminal: RuntimeSession?) {
        self.task = task
        self.repositories = repositories.filter { task.repositoryIds.contains($0.id) }
        self.terminal = terminal
    }

    public var body: some View {
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

/// The title of a task, struck through once it is done.
public struct RoomTaskTitle: View {
    @Environment(\.theme) private var theme
    let task: RoomTask
    let lines: Int?

    public init(task: RoomTask, lines: Int?) {
        self.task = task
        self.lines = lines
    }

    public var body: some View {
        Text(task.title)
            .appTextStyle(.body)
            .fontWeight(.medium)
            .foregroundStyle(task.closed ? theme.colors.inkMuted : theme.colors.ink)
            .strikethrough(task.closed, color: theme.colors.inkFaint)
            .multilineTextAlignment(.leading)
            .lineLimit(lines)
    }
}

/// The context of a task, with its links.
public struct RoomTaskDescription: View {
    #if os(iOS)
        private static let lineSpacing: CGFloat = 2
    #else
        private static let lineSpacing: CGFloat = 2.5
    #endif

    @Environment(\.theme) private var theme
    let text: String

    public init(text: String) {
        self.text = text
    }

    public var body: some View {
        Text(RoomMessageText.formatted(text))
            .appTextStyle(.body)
            .lineSpacing(Self.lineSpacing)
            .foregroundStyle(theme.colors.inkMuted)
            .tint(theme.colors.accentStrong)
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// The title of a lane of open tasks, with how many it holds.
public struct RoomTaskLaneTitle: View {
    public enum Lane: Sendable {
        case upForGrabs
        case inProgress
    }

    @Environment(\.theme) private var theme
    let lane: Lane
    let count: Int

    public init(_ lane: Lane, count: Int) {
        self.lane = lane
        self.count = count
    }

    public var body: some View {
        HStack(spacing: 6) {
            Group {
                switch lane {
                case .upForGrabs: Text("Up for grabs", bundle: .module)
                case .inProgress: Text("In progress", bundle: .module)
                }
            }
            .appTextStyle(.subhead)
            .foregroundStyle(theme.colors.inkMuted)
            Text(count, format: .number)
                .appTextStyle(.footnote)
                .monospacedDigit()
                .foregroundStyle(theme.colors.inkFaint)
                .contentTransition(.numericText())
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// The key that shows or hides the done tasks, with how many there are.
public struct RoomDoneTasksKey: View {
    #if os(iOS)
        private static let height: CGFloat = 32
    #else
        private static let height: CGFloat = 20
    #endif

    @Environment(\.theme) private var theme
    @ScaledMetric(relativeTo: .subheadline) private var chevron: CGFloat = 8
    let state: RoomViewState
    let count: Int
    let identifier: String

    public init(state: RoomViewState, count: Int, identifier: String) {
        self.state = state
        self.count = count
        self.identifier = identifier
    }

    public var body: some View {
        Button {
            withAnimation(theme.motion.spring) { state.showsDone.toggle() }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "chevron.right")
                    .font(.system(size: chevron, weight: .bold))
                    .rotationEffect(.degrees(state.showsDone ? 90 : 0))
                Text("Done", bundle: .module).appTextStyle(.subhead)
                Text(count, format: .number).appTextStyle(.footnote).monospacedDigit().contentTransition(.numericText())
                Spacer(minLength: 0)
            }
            .foregroundStyle(theme.colors.inkMuted)
            .frame(minHeight: Self.height)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(state.showsDone ? Text("Shown", bundle: .module) : Text("Hidden", bundle: .module))
        .accessibilityIdentifier(identifier)
    }
}

public extension View {
    /// Marks a task that you work on with a copper edge.
    func roomTaskMarker(_ shown: Bool) -> some View {
        modifier(RoomTaskMarker(shown: shown))
    }
}

private struct RoomTaskMarker: ViewModifier {
    @Environment(\.theme) private var theme
    let shown: Bool

    func body(content: Content) -> some View {
        content.overlay(alignment: .leading) {
            if shown {
                Capsule().fill(theme.colors.accent).frame(width: 3).padding(.vertical, 12).padding(.leading, -1)
            }
        }
    }
}
