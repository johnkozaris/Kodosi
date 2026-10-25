import KodosiTerminal
import SwiftUI

enum TerminalRows {
    static let minimum = 10

    static func height(room: CGFloat, metrics: TerminalRowMetrics?, scale: CGFloat) -> CGFloat {
        guard let metrics, metrics.cellHeightPixels > 0, scale > 0 else { return room }
        let pixels = CGFloat(minimum) * CGFloat(metrics.cellHeightPixels) + CGFloat(metrics.insetPixels)
        return max(room, pixels / scale)
    }

    static func top(room: CGFloat, height: CGFloat, cursor: ClosedRange<CGFloat>?) -> CGFloat {
        let hidden = max(0, height - room)
        guard let cursor, cursor.lowerBound < hidden else { return hidden }
        return min(hidden, max(0, cursor.upperBound - room))
    }
}

struct TerminalRowsFrame<Content: View>: View {
    @ObservedObject private var renderer: TerminalRendererSession
    @ObservedObject private var cursor: TerminalCursor
    @Environment(\.displayScale) private var scale
    private let content: Content

    init(renderer: TerminalRendererSession, @ViewBuilder content: () -> Content) {
        _renderer = ObservedObject(wrappedValue: renderer)
        _cursor = ObservedObject(wrappedValue: renderer.cursor)
        self.content = content()
    }

    var body: some View {
        GeometryReader { proxy in
            let height = TerminalRows.height(room: proxy.size.height, metrics: renderer.rowMetrics, scale: scale)
            content
                .frame(width: proxy.size.width, height: height)
                .offset(y: -TerminalRows.top(room: proxy.size.height, height: height, cursor: cursor.row))
                .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
        }
        .clipped()
    }
}
