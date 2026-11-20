import CoreGraphics
import Foundation

/// A touch screen counts no clicks, so taps that follow each other in about one place count as
/// the clicks of a mouse: two select a word, and three a paragraph.
public struct BrowserTaps: Sendable {
    private struct Tap {
        let time: TimeInterval
        let point: CGPoint
        let clicks: Int
    }

    private static let interval: TimeInterval = 0.5
    private static let reach: CGFloat = 44

    private var last: Tap?

    public init() {}

    public mutating func click(at point: CGPoint, time: TimeInterval) -> Int {
        var clicks = 1
        if let last, time - last.time <= Self.interval,
           hypot(point.x - last.point.x, point.y - last.point.y) <= Self.reach
        {
            clicks = last.clicks + 1
        }
        last = Tap(time: time, point: point, clicks: clicks)
        return clicks
    }

    public mutating func forget() {
        last = nil
    }
}
