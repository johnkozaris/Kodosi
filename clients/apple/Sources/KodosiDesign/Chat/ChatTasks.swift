import KodosiBridge
import SwiftUI

struct ChatTasks: View {
    @Environment(\.theme) private var theme
    let tasks: [ChatTask]
    @State private var open = false

    var body: some View {
        let done = tasks.count { $0.state == .done }
        let current = tasks.first { $0.state == .working } ?? tasks.first { $0.state == .pending }
        VStack(alignment: .leading, spacing: 4) {
            Button { open.toggle() } label: {
                HStack(spacing: 7) {
                    Image(systemName: "checklist").font(mac: 10, ios: .caption, weight: .semibold).foregroundStyle(theme.colors.inkMuted)
                        .accessibilityHidden(true)
                    Text(verbatim: "\(done)/\(tasks.count)").font(mac: 11, ios: .caption, design: .monospaced).monospacedDigit()
                        .foregroundStyle(theme.colors.inkMuted)
                    ZStack(alignment: .leading) {
                        if !open, let current {
                            Text(verbatim: current.text).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                                .lineLimit(1).truncationMode(.tail)
                                .id(current.text)
                                .transition(AnyTransition.roll)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .clipped()
                    Image(systemName: "chevron.up").font(mac: 8, ios: .caption2, weight: .bold)
                        .rotationEffect(.degrees(open ? 180 : 0)).foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
                .controlHeight(22).touchTarget(Rectangle())
            }
            .buttonStyle(.plain)
            .help(open ? Text("Hide the tasks", bundle: .module) : Text("Show the tasks", bundle: .module))
            .accessibilityLabel(Text("Tasks: \(done) of \(tasks.count) done", bundle: .module))
            .accessibilityHint(open ? Text("Hide the tasks", bundle: .module) : Text("Show the tasks", bundle: .module))
            .accessibilityIdentifier("chat.tasks")
            if open {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(tasks.indices, id: \.self) { index in
                        ChatTaskLine(task: tasks[index])
                    }
                }
                .padding(.leading, 2).padding(.bottom, 4)
                .transition(AnyTransition.unfold)
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 2)
        .background { BezelBackground(shape: RoundedRectangle(cornerRadius: 13, style: .continuous), fill: theme.colors.raised, bezel: .quiet) }
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.snappy, value: current?.text)
        .animation(theme.motion.snappy, value: done)
    }
}

private struct ChatTaskLine: View {
    @Environment(\.theme) private var theme
    let task: ChatTask

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 7) {
            Group {
                switch task.state {
                case .done:
                    Image(systemName: "checkmark").foregroundStyle(theme.colors.ready)
                        .accessibilityLabel(Text("Done", bundle: .module))
                case .working:
                    Image(systemName: "circle.fill").foregroundStyle(theme.colors.ink)
                        .accessibilityHidden(true)
                case .pending:
                    Image(systemName: "circle").foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
            }
            .font(mac: 8, ios: .caption2, weight: .bold)
            .frame(width: 12)
            .contentTransition(.symbolEffect(.replace))
            Text(verbatim: task.text).appTextStyle(.footnote)
                .foregroundStyle(task.state == .working ? theme.colors.ink : theme.colors.inkMuted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .animation(theme.motion.snappy, value: task.state)
        .accessibilityElement(children: .combine)
    }
}
