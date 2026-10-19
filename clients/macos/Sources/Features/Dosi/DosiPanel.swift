import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiPanel: View {
    @Environment(AppDependencies.self) private var deps
    @FocusState private var composing: Bool

    var body: some View {
        VStack(spacing: 0) {
            DosiShelf().padding(.horizontal, 12).padding(.top, 10)
            DosiThread(listening: composing).arrive(delay: 0.12, rise: 8)
            DosiComposer(focused: $composing)
        }
        .onAppear { deps.dosi.see() }
        .onChange(of: deps.dosi.items.last?.id) { _, _ in deps.dosi.see() }
    }
}

extension DosiStore {
    var touched: [DosiTarget] {
        guard busy, let turn = items.last?.turn else { return [] }
        var targets: [DosiTarget] = []
        for item in items.reversed() where item.turn == turn {
            guard case let .tool(tool) = item.body, let target = tool.target, [.terminal, .room].contains(target.kind),
                  !targets.contains(target) else { continue }
            targets.append(target)
            if targets.count == 3 {
                break
            }
        }
        return targets.reversed()
    }

    var reading: DosiItem? {
        guard busy, let item = items.last, case let .tool(tool) = item.body, tool.status == .running,
              [.look, .read, .wait].contains(tool.action), tool.target != nil else { return nil }
        return item
    }
}

extension DosiTarget {
    var key: String {
        "\(kind.rawValue):\(value)"
    }
}
