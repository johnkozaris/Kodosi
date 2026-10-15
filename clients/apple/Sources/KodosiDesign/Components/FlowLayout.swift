import SwiftUI

public struct FillsRow: LayoutValueKey {
    public static let defaultValue: CGFloat? = nil
}

public struct FlowLayout: Layout {
    let spacing: CGFloat

    public init(spacing: CGFloat = 6) {
        self.spacing = spacing
    }

    public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        let rows = rows(width: proposal.width ?? .infinity, subviews: subviews)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: width, height: height)
    }

    public func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        var y = bounds.minY
        for row in rows(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for (index, size) in row.items {
                let top = subviews[index][FillsRow.self] == nil ? (row.line - size.height) / 2 : 0
                subviews[index].place(at: CGPoint(x: x, y: y + top), anchor: .topLeading, proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row {
        var items: [(index: Int, size: CGSize)] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
        var line: CGFloat = 0
    }

    private func rows(width: CGFloat, subviews: Subviews) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for index in subviews.indices {
            let subview = subviews[index]
            let used = row.items.isEmpty ? 0 : row.width + spacing
            var size: CGSize
            if let minimum = subview[FillsRow.self], width.isFinite {
                if width - used < minimum, !row.items.isEmpty {
                    rows.append(row)
                    row = Row()
                }
                let start = row.items.isEmpty ? 0 : row.width + spacing
                size = subview.sizeThatFits(ProposedViewSize(width: width - start, height: nil))
            } else {
                size = subview.sizeThatFits(.unspecified)
                if size.width > width {
                    size = subview.sizeThatFits(ProposedViewSize(width: width, height: nil))
                }
                if used + size.width > width, !row.items.isEmpty {
                    rows.append(row)
                    row = Row()
                }
            }
            row.width = row.items.isEmpty ? size.width : row.width + spacing + size.width
            row.height = max(row.height, size.height)
            if subview[FillsRow.self] == nil {
                row.line = max(row.line, size.height)
            }
            row.items.append((index, size))
        }
        if !row.items.isEmpty {
            rows.append(row)
        }
        return rows
    }
}
