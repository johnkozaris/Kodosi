import KodosiTerminal
import SwiftUI

enum TerminalRows {
    static let minimum = 10

    static func height(room: CGFloat, metrics: TerminalRowMetrics?, scale: CGFloat) -> CGFloat {
        guard let metrics, metrics.cellHeightPixels > 0, scale > 0 else { return room }
        let pixels = CGFloat(minimum) * CGFloat(metrics.cellHeightPixels) + CGFloat(metrics.insetPixels)
        return max(room, pixels / scale)
    }
}

struct TerminalRowsFrame<Content: View>: View {
    @ObservedObject private var renderer: TerminalRendererSession
    @Environment(\.displayScale) private var scale
    private let content: Content

    init(renderer: TerminalRendererSession, @ViewBuilder content: () -> Content) {
        _renderer = ObservedObject(wrappedValue: renderer)
        self.content = content()
    }

    var body: some View {
        GeometryReader { proxy in
            content
                .frame(width: proxy.size.width, height: TerminalRows.height(room: proxy.size.height, metrics: renderer.rowMetrics, scale: scale))
                .frame(width: proxy.size.width, height: proxy.size.height, alignment: .bottom)
        }
        .clipped()
    }
}
