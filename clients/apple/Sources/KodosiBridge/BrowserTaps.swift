import CoreGraphics
import Foundation

/// Synthesizes mouse click counts from consecutive nearby taps (for word and paragraph selection)
/// because touch events do not track multi-click counts.
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
