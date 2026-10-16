import CoreGraphics
import Foundation

/// The color of a cell: the strong copper of a fresh keystroke, or a mix from copper (0) to the soft track (1).
enum CellInk: Hashable, Sendable {
    case strong
    case mix(Double)

    static func cooling(_ age: TimeInterval) -> CellInk {
        switch age {
        case ..<0.14: .strong
        case ..<0.45: .mix(0)
        case ..<0.9: .mix(0.45)
        case ..<1.4: .mix(0.72)
        default: .mix(1)
        }
    }

    static func filling(_ reach: Double) -> CellInk {
        reach >= 1 ? .mix(0) : reach <= 0 ? .mix(1) : .mix(1 - reach)
    }
}

struct CellKey<Value: Equatable & Sendable>: Equatable, Sendable {
    var time: TimeInterval
    var value: Value
}

/// One period of a rim or line that types: the keys of each cell and of the cursor, which is nil while it is dark.
struct CellTrackFrames: Equatable, Sendable {
    var period: TimeInterval
    var cells: [[CellKey<CellInk>]]
    var cursor: [CellKey<Int?>]

    /// The cursor types round the rim, and each cell cools after the cursor leaves it.
    static func rim(count: Int) -> CellTrackFrames {
        let steps = TypingSteps(lap: count)
        let cells = (0 ..< count).map { cell in
            keys(lit: steps.times(of: cell, lap: count), resets: [], period: steps.period)
        }
        var cursor: [CellKey<Int?>] = []
        for (index, start) in steps.times.enumerated() {
            let end = steps.end(of: index)
            let cell = steps.heads[index] % count
            cursor.append(CellKey(time: start, value: cell))
            var blink = start + 0.24
            var lit = false
            while blink < end {
                cursor.append(CellKey(time: blink, value: lit ? cell : nil))
                blink += 0.42
                lit.toggle()
            }
        }
        return CellTrackFrames(period: steps.period, cells: cells, cursor: merged(cursor))
    }

    /// The cursor types along the line, waits six beats past its end and starts again on a clear line.
    static func line(count: Int) -> CellTrackFrames {
        let lap = count + 6
        let steps = TypingSteps(lap: lap)
        let resets = steps.times(of: 0, lap: lap)
        let cells = (0 ..< count).map { cell in
            keys(lit: steps.times(of: cell, lap: lap), resets: cell == 0 ? [] : resets, period: steps.period)
        }
        let cursor = steps.times.indices.map { index in
            let reached = steps.heads[index] % lap
            return CellKey<Int?>(time: steps.times[index], value: reached < count ? reached : nil)
        }
        return CellTrackFrames(period: steps.period, cells: cells, cursor: merged(cursor))
    }

    /// The still rim of reduced motion: the cursor rests on the first cell with a short trail behind it.
    static func stillRim(count: Int) -> [CellInk] {
        (0 ..< count).map { cell in
            let ago = (count - cell) % count
            return .cooling(ago < 4 ? Double(ago) * 0.25 : 9)
        }
    }

    static func progress(count: Int, fraction: Double) -> [CellInk] {
        (0 ..< count).map { .filling(fraction * Double(count) - Double($0)) }
    }

    func inks(at time: TimeInterval) -> [CellInk] {
        cells.map { Self.value(of: $0, at: time) ?? .mix(1) }
    }

    func cursor(at time: TimeInterval) -> Int? {
        Self.value(of: cursor, at: time) ?? nil
    }

    private static func value<Value>(of keys: [CellKey<Value>], at time: TimeInterval) -> Value? {
        keys.last { $0.time <= time }?.value
    }

    private static func keys(lit: [TimeInterval], resets: [TimeInterval], period: TimeInterval) -> [CellKey<CellInk>] {
        let events = (lit.map { (time: $0, lit: true) } + resets.map { (time: $0, lit: false) }).sorted { $0.time < $1.time }
        guard let last = events.last else { return [CellKey(time: 0, value: .mix(1))] }
        func ink(at time: TimeInterval) -> CellInk {
            let event = events.last { $0.time <= time } ?? (time: last.time - period, lit: last.lit)
            return event.lit ? .cooling(time - event.time) : .mix(1)
        }
        var changes: Set<TimeInterval> = [0]
        for event in events {
            changes.insert(event.time)
            if event.lit {
                for age in [0.14, 0.45, 0.9, 1.4] {
                    changes.insert((event.time + age).truncatingRemainder(dividingBy: period))
                }
            }
        }
        var keys: [CellKey<CellInk>] = []
        for time in changes.sorted() {
            let value = ink(at: time + 1e-6)
            if keys.last?.value != value {
                keys.append(CellKey(time: time, value: value))
            }
        }
        return keys
    }

    private static func merged<Value>(_ keys: [CellKey<Value>]) -> [CellKey<Value>] {
        keys.reduce(into: []) { result, key in
            if result.last?.value != key.value {
                result.append(key)
            }
        }
    }
}

/// The keystrokes of one period: a whole number of laps of at least 64 strokes, so that the period repeats seamlessly.
private struct TypingSteps {
    let times: [TimeInterval]
    let heads: [Int]
    let period: TimeInterval

    init(lap: Int) {
        let total = lap * Int((64 / Double(lap)).rounded(.up))
        var offsets: [TimeInterval] = []
        var sum: TimeInterval = 0
        for gap in TypingCadence.gaps(total) {
            sum += gap
            offsets.append(sum)
        }
        period = sum
        times = [0] + offsets.dropLast()
        heads = [total - 1] + Array(0 ..< total - 1)
    }

    func end(of index: Int) -> TimeInterval {
        index + 1 < times.count ? times[index + 1] : period
    }

    func times(of cell: Int, lap: Int) -> [TimeInterval] {
        times.indices.filter { heads[$0] % lap == cell }.map { times[$0] }
    }
}

/// The rhythm of a person typing: bursts of quick keys between short pauses, the same on every device.
enum TypingCadence {
    static func gaps(_ count: Int) -> [TimeInterval] {
        var state: UInt64 = 0x2545_F491_4F6C_DD1D
        func next() -> Double {
            state = state &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
            return Double(state >> 11) / Double(UInt64(1) << 53)
        }
        var gaps: [TimeInterval] = []
        var burst = 0
        while gaps.count < count {
            if burst == 0 {
                burst = 5 + Int(next() * 9)
                gaps.append(0.3 + next() * 0.45)
            } else {
                gaps.append(0.045 + next() * 0.07)
            }
            burst -= 1
        }
        return gaps
    }
}

/// Where the cells and the cursor of a rim or a line sit, in points from the top left of the layer.
struct CellTrackShape: Equatable, Sendable {
    var cells: [CGRect]
    var cellRadius: CGFloat
    var cursors: [CGRect]
    var cursorRadius: CGFloat

    static func rim(in rect: CGRect, cornerRadius: CGFloat?, thickness: CGFloat, scale: CGFloat) -> CellTrackShape? {
        let track = RimTrack(size: rect.size, cornerRadius: cornerRadius, inset: thickness / 2)
        guard track.length > 0 else { return nil }
        let count = max(8, Int(track.length / (thickness * 2.5)))
        let length = track.length / CGFloat(count) * 0.6
        let places = (0 ..< count).map { index in
            let (point, angle) = track.point(at: (CGFloat(index) + 0.5) / CGFloat(count))
            return (CGPoint(x: point.x + rect.minX, y: point.y + rect.minY), angle)
        }
        return CellTrackShape(places: places, length: length, thickness: thickness, scale: scale)
    }

    static func line(in rect: CGRect, thickness: CGFloat, scale: CGFloat) -> CellTrackShape? {
        guard rect.width > 0, thickness > 0 else { return nil }
        let count = max(3, Int(rect.width / (thickness * 2.4)))
        let pitch = rect.width / CGFloat(count)
        let places = (0 ..< count).map { index in
            (CGPoint(x: rect.minX + (CGFloat(index) + 0.5) * pitch, y: rect.midY), CGFloat(0))
        }
        return CellTrackShape(places: places, length: pitch * 0.6, thickness: thickness, scale: scale)
    }

    private init(places: [(CGPoint, CGFloat)], length: CGFloat, thickness: CGFloat, scale: CGFloat) {
        let side = (length + thickness) / 2
        cells = places.map { point, angle in
            let across = abs(cos(angle)) >= abs(sin(angle))
            let corner = abs(abs(cos(angle)) - abs(sin(angle))) < 0.4
            let size = corner ? CGSize(width: side, height: side)
                : across ? CGSize(width: length, height: thickness) : CGSize(width: thickness, height: length)
            return Self.snapped(point, size, scale)
        }
        let cursor = CGSize(width: max(thickness * 1.5, length), height: thickness * 2.6)
        cursors = places.map { Self.snapped($0.0, cursor, scale) }
        cellRadius = thickness * 0.3
        cursorRadius = thickness * 0.35
    }

    private static func snapped(_ center: CGPoint, _ size: CGSize, _ scale: CGFloat) -> CGRect {
        let scale = max(scale, 1)
        return CGRect(
            x: ((center.x - size.width / 2) * scale).rounded() / scale, y: ((center.y - size.height / 2) * scale).rounded() / scale,
            width: (size.width * scale).rounded() / scale, height: (size.height * scale).rounded() / scale
        )
    }
}

private struct RimTrack {
    private enum Piece {
        case line(CGPoint, CGPoint)
        case arc(CGPoint, CGFloat)
    }

    private let pieces: [Piece]
    private let radius: CGFloat
    private let start: CGPoint
    let length: CGFloat

    init(size: CGSize, cornerRadius: CGFloat?, inset: CGFloat) {
        let rect = CGRect(origin: .zero, size: size).insetBy(dx: inset, dy: inset)
        let limit = max(0, min(rect.width, rect.height) / 2)
        let radius = min(max((cornerRadius ?? limit) - inset, 0), limit)
        self.radius = radius
        let (left, right, top, bottom) = (rect.minX, rect.maxX, rect.minY, rect.maxY)
        start = CGPoint(x: rect.midX, y: top)
        pieces = [
            .line(CGPoint(x: rect.midX, y: top), CGPoint(x: right - radius, y: top)),
            .arc(CGPoint(x: right - radius, y: top + radius), -.pi / 2),
            .line(CGPoint(x: right, y: top + radius), CGPoint(x: right, y: bottom - radius)),
            .arc(CGPoint(x: right - radius, y: bottom - radius), 0),
            .line(CGPoint(x: right - radius, y: bottom), CGPoint(x: left + radius, y: bottom)),
            .arc(CGPoint(x: left + radius, y: bottom - radius), .pi / 2),
            .line(CGPoint(x: left, y: bottom - radius), CGPoint(x: left, y: top + radius)),
            .arc(CGPoint(x: left + radius, y: top + radius), .pi),
            .line(CGPoint(x: left + radius, y: top), CGPoint(x: rect.midX, y: top)),
        ]
        length = max(0, 2 * (rect.width - 2 * radius) + 2 * (rect.height - 2 * radius) + 2 * .pi * radius)
    }

    func point(at fraction: CGFloat) -> (CGPoint, CGFloat) {
        var rest = (fraction - fraction.rounded(.down)) * length
        for piece in pieces {
            switch piece {
            case let .line(start, end):
                let span = hypot(end.x - start.x, end.y - start.y)
                if rest <= span, span > 0 {
                    let part = rest / span
                    return (CGPoint(x: start.x + (end.x - start.x) * part, y: start.y + (end.y - start.y) * part),
                            atan2(end.y - start.y, end.x - start.x))
                }
                rest -= span
            case let .arc(center, start):
                let span = .pi / 2 * radius
                if rest <= span, span > 0 {
                    let angle = start + rest / radius
                    return (CGPoint(x: center.x + cos(angle) * radius, y: center.y + sin(angle) * radius), angle + .pi / 2)
                }
                rest -= span
            }
        }
        return (start, 0)
    }
}
