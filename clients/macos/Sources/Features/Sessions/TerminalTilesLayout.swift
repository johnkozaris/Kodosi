import SwiftUI

struct TerminalTileVisibility: LayoutValueKey {
    static let defaultValue = true
}

struct TerminalTilesLayout: Layout {
    func makeCache(subviews _: Subviews) -> [Int: CGSize] {
        [:]
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews _: Subviews, cache _: inout [Int: CGSize]) -> CGSize {
        proposal.replacingUnspecifiedDimensions(by: CGSize(width: 800, height: 600))
    }

    func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache: inout [Int: CGSize]) {
        let visible = subviews.indices.filter { subviews[$0][TerminalTileVisibility.self] }
        let frames = Self.frames(count: visible.count, in: bounds)
        for (index, frame) in zip(visible, frames) {
            cache[index] = frame.size
            subviews[index].place(at: frame.origin, anchor: .topLeading, proposal: ProposedViewSize(frame.size))
        }
        for index in subviews.indices where !subviews[index][TerminalTileVisibility.self] {
            let size = cache[index] ?? bounds.size
            subviews[index].place(at: CGPoint(x: bounds.maxX + 10000, y: bounds.maxY + 10000), anchor: .topLeading,
                                  proposal: ProposedViewSize(size))
        }
    }

    static func frames(count: Int, in bounds: CGRect) -> [CGRect] {
        guard count > 0 else { return [] }
        let gap: CGFloat = 6
        let maxColumns = min(count, max(1, Int((bounds.width + gap) / 400)))
        let columns = (1 ... maxColumns).min { lhs, rhs in
            func penalty(_ columns: Int) -> CGFloat {
                let rows = (count + columns - 1) / columns
                let width = max(1, (bounds.width - CGFloat(columns - 1) * gap) / CGFloat(columns))
                let height = max(1, (bounds.height - CGFloat(rows - 1) * gap) / CGFloat(rows))
                return abs(log(width / height / 1.5))
            }
            return penalty(lhs) < penalty(rhs)
        } ?? 1
        let rows = (count + columns - 1) / columns
        let height = max(1, (bounds.height - CGFloat(rows - 1) * gap) / CGFloat(rows))
        return (0 ..< count).map { index in
            let row = index / columns
            let rowCount = min(columns, count - row * columns)
            let width = max(1, (bounds.width - CGFloat(rowCount - 1) * gap) / CGFloat(rowCount))
            return CGRect(x: bounds.minX + CGFloat(index % columns) * (width + gap),
                          y: bounds.minY + CGFloat(row) * (height + gap), width: width, height: height)
        }
    }
}
