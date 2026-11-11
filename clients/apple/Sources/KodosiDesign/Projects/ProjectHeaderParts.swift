import KodosiBridge
import SwiftUI

public struct ProjectFacts: View {
    @Environment(\.theme) private var theme
    @ScaledMetric(relativeTo: .caption) private var lock: CGFloat = 8
    let people: Int?
    let terminals: Int

    public init(people: Int?, terminals: Int) {
        self.people = people
        self.terminals = terminals
    }

    public var body: some View {
        HStack(spacing: 5) {
            Image(systemName: "lock.fill").font(.system(size: lock, weight: .semibold))
            Text(verbatim: [people.map(Counted.people), Counted.terminals(terminals)].compactMap(\.self).joined(separator: " · "))
                .contentTransition(.numericText())
        }
        .appTextStyle(.caption)
        .fontWeight(.regular)
        .foregroundStyle(theme.colors.inkFaint)
        .lineLimit(1)
    }
}

public extension DockItem {
    static func projectTasks(id: Int, tasks: [ProjectTask], fresh: Bool) -> DockItem {
        let open = tasks.count { !$0.closed }
        return DockItem(
            id: id, title: String(localized: "Tasks", bundle: .module), symbol: "checklist", count: open,
            progress: tasks.isEmpty ? nil : Double(tasks.count - open) / Double(tasks.count), fresh: fresh,
        )
    }
}
