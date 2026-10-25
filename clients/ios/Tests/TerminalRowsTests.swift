import CoreGraphics
@testable import Kodosi
import KodosiTerminal
import Testing

struct TerminalRowsTests {
    private let scale: CGFloat = 3
    private let cellPixels: UInt32 = 53
    private let paddingPixels: CGFloat = 32

    private func measure(_ height: CGFloat) -> TerminalRowMetrics {
        let pixels = height * scale
        let rows = UInt16(max(0, ((pixels - paddingPixels) / CGFloat(cellPixels)).rounded(.down)))
        return TerminalRowMetrics(rows: rows, heightPixels: UInt32(pixels.rounded()), cellHeightPixels: cellPixels)
    }

    private func rows(in height: CGFloat) -> Int {
        Int(((height * scale - paddingPixels) / CGFloat(cellPixels)).rounded(.down))
    }

    @Test func `a phone terminal with little room keeps ten rows`() {
        #expect(rows(in: 44) == 1)
        let height = TerminalRows.height(room: 44, metrics: measure(44), scale: scale)
        #expect(rows(in: height) == TerminalRows.minimum)
        #expect(TerminalRows.height(room: 44, metrics: measure(height), scale: scale) == height)
    }

    @Test func `a terminal that loses its room keeps ten rows in its first layout`() {
        let height = TerminalRows.height(room: 44, metrics: measure(326), scale: scale)
        #expect(rows(in: height) == TerminalRows.minimum)
        #expect(TerminalRows.height(room: 44, metrics: measure(height), scale: scale) == height)
    }

    @Test func `a phone terminal with room for more rows keeps its room`() {
        #expect(TerminalRows.height(room: 326, metrics: measure(326), scale: scale) == 326)
        #expect(rows(in: 326) == 17)
    }

    @Test func `a terminal that has not measured its cells keeps its room`() {
        #expect(TerminalRows.height(room: 44, metrics: nil, scale: scale) == 44)
    }
}
