import SwiftUI

private struct Shimmer: ViewModifier {
    @Environment(\.theme) private var theme
    @State private var passing = false
    let active: Bool

    func body(content: Content) -> some View {
        if active, !theme.motion.reduced {
            content.overlay {
                GeometryReader { geometry in
                    let band = max(geometry.size.width * 0.5, 40)
                    LinearGradient(
                        colors: [.clear, theme.colors.ink, .clear],
                        startPoint: .leading, endPoint: .trailing
                    )
                    .frame(width: band)
                    .offset(x: passing ? geometry.size.width : -band)
                }
                .mask(content)
                .allowsHitTesting(false)
            }
            .onAppear {
                withAnimation(.easeInOut(duration: 1.9).repeatForever(autoreverses: false)) { passing = true }
            }
        } else {
            content
        }
    }
}

public struct CellRim: View {
    public enum Mode: Equatable, Sendable {
        case working
        case progress(Double)
    }

    @Environment(\.theme) private var theme
    public let mode: Mode
    public var cornerRadius: CGFloat?
    public var lineWidth: CGFloat

    public init(_ mode: Mode, cornerRadius: CGFloat? = nil, lineWidth: CGFloat = 2) {
        self.mode = mode
        self.cornerRadius = cornerRadius
        self.lineWidth = lineWidth
    }

    public var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: theme.motion.reduced)) { context in
            let time = theme.motion.reduced ? 0 : context.date.timeIntervalSinceReferenceDate
            Canvas { canvas, size in
                draw(in: canvas, size: size, time: time)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func draw(in canvas: GraphicsContext, size: CGSize, time: TimeInterval) {
        let track = RimTrack(size: size, cornerRadius: cornerRadius, inset: lineWidth / 2)
        guard track.length > 0 else { return }
        let count = max(8, Int(track.length / (lineWidth * 2.5)))
        let cells = CellInk(colors: theme.colors, scale: canvas.environment.displayScale, thickness: lineWidth,
                            length: track.length / CGFloat(count) * 0.6)
        let reduced = theme.motion.reduced
        let head = reduced ? 0 : TypingCadence.step(at: time)
        var cursor: (index: Int, lit: Bool)?
        for index in 0 ..< count {
            let color: Color
            switch mode {
            case let .progress(fraction):
                let reach = CGFloat(fraction) * CGFloat(count) - CGFloat(index)
                color = cells.filling(reach)
            case .working:
                let ago = wrapped(head - index, count)
                color = reduced ? cells.cooling(ago < 4 ? Double(ago) * 0.25 : 9) : cells.cooling(time - TypingCadence.time(of: head - ago))
            }
            cells.fill(canvas, track.point(at: (CGFloat(index) + 0.5) / CGFloat(count)), color: color)
        }
        switch mode {
        case .working:
            let still = time - TypingCadence.time(of: head)
            cursor = (wrapped(head, count), reduced || still < 0.24 || Int((still - 0.24) / 0.42) % 2 == 1)
        case let .progress(fraction) where fraction < 1:
            cursor = (min(count - 1, Int(CGFloat(fraction) * CGFloat(count))), reduced || Int(time / 0.53) % 2 == 0)
        case .progress:
            cursor = nil
        }
        if let cursor, cursor.lit {
            cells.cursor(canvas, at: track.point(at: (CGFloat(cursor.index) + 0.5) / CGFloat(count)).0)
        }
    }
}

struct CellLine: View {
    enum Mode: Equatable, Sendable {
        case waiting, flowing, done
    }

    @Environment(\.theme) private var theme
    let mode: Mode
    var lineWidth: CGFloat

    init(_ mode: Mode, lineWidth: CGFloat = 3) {
        self.mode = mode
        self.lineWidth = lineWidth
    }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: theme.motion.reduced || mode != .flowing)) { context in
            let time = theme.motion.reduced ? 0 : context.date.timeIntervalSinceReferenceDate
            Canvas { canvas, size in
                let count = max(3, Int(size.width / (lineWidth * 2.4)))
                let pitch = size.width / CGFloat(count)
                let cells = CellInk(colors: theme.colors, scale: canvas.environment.displayScale, thickness: lineWidth,
                                    length: pitch * 0.6)
                let head = TypingCadence.step(at: time)
                let lap = count + 6
                let reached = wrapped(head, lap)
                for index in 0 ..< count {
                    let color = switch mode {
                    case .done: theme.colors.accent
                    case .waiting: theme.colors.accentSoft
                    case .flowing: index <= reached
                        ? cells.cooling(time - TypingCadence.time(of: head - (reached - index))) : theme.colors.accentSoft
                    }
                    cells.fill(canvas, (CGPoint(x: (CGFloat(index) + 0.5) * pitch, y: size.height / 2), 0), color: color)
                }
                if mode == .flowing, reached < count {
                    cells.cursor(canvas, at: CGPoint(x: (CGFloat(reached) + 0.5) * pitch, y: size.height / 2))
                }
            }
        }
        .frame(height: lineWidth * 3)
        .animation(theme.motion.fade, value: mode)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

private struct CellInk {
    let colors: AppTheme.Colors
    let scale: CGFloat
    let thickness: CGFloat
    let length: CGFloat

    func cooling(_ age: TimeInterval) -> Color {
        switch age {
        case ..<0.14: colors.accentStrong
        case ..<0.45: colors.accent
        case ..<0.9: colors.accent.mix(with: colors.accentSoft, by: 0.45)
        case ..<1.4: colors.accent.mix(with: colors.accentSoft, by: 0.72)
        default: colors.accentSoft
        }
    }

    func filling(_ reach: CGFloat) -> Color {
        reach >= 1 ? colors.accent : reach <= 0 ? colors.accentSoft : colors.accent.mix(with: colors.accentSoft, by: 1 - reach)
    }

    func fill(_ canvas: GraphicsContext, _ place: (CGPoint, CGFloat), color: Color) {
        let (point, angle) = place
        let across = abs(cos(angle)) >= abs(sin(angle))
        let corner = abs(abs(cos(angle)) - abs(sin(angle))) < 0.4
        let side = (length + thickness) / 2
        let size = corner ? CGSize(width: side, height: side)
            : across ? CGSize(width: length, height: thickness) : CGSize(width: thickness, height: length)
        canvas.fill(Path(roundedRect: snapped(point, size), cornerRadius: thickness * 0.3), with: .color(color))
    }

    func cursor(_ canvas: GraphicsContext, at point: CGPoint) {
        let size = CGSize(width: max(thickness * 1.5, length), height: thickness * 2.6)
        canvas.fill(Path(roundedRect: snapped(point, size), cornerRadius: thickness * 0.35), with: .color(colors.accentStrong))
    }

    private func snapped(_ center: CGPoint, _ size: CGSize) -> CGRect {
        let scale = max(scale, 1)
        return CGRect(
            x: ((center.x - size.width / 2) * scale).rounded() / scale, y: ((center.y - size.height / 2) * scale).rounded() / scale,
            width: (size.width * scale).rounded() / scale, height: (size.height * scale).rounded() / scale
        )
    }
}

enum TypingCadence {
    private static let gaps: [TimeInterval] = {
        var state: UInt64 = 0x2545_F491_4F6C_DD1D
        func next() -> Double {
            state = state &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
            return Double(state >> 11) / Double(UInt64(1) << 53)
        }
        var gaps: [TimeInterval] = []
        var burst = 0
        while gaps.count < 64 {
            if burst == 0 {
                burst = 5 + Int(next() * 9)
                gaps.append(0.3 + next() * 0.45)
            } else {
                gaps.append(0.045 + next() * 0.07)
            }
            burst -= 1
        }
        return gaps
    }()

    private static let offsets: [TimeInterval] = gaps.reduce(into: []) { $0.append(($0.last ?? 0) + $1) }
    private static let period = offsets[offsets.count - 1]

    static func step(at time: TimeInterval) -> Int {
        let round = (time / period).rounded(.down)
        let within = time - round * period
        let index = offsets.firstIndex { $0 > within } ?? offsets.count
        return Int(round) * offsets.count + index - 1
    }

    static func time(of step: Int) -> TimeInterval {
        let round = (Double(step) / Double(offsets.count)).rounded(.down)
        return round * period + offsets[wrapped(step, offsets.count)]
    }
}

private func wrapped(_ value: Int, _ count: Int) -> Int {
    ((value % count) + count) % count
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

public struct BreathingDot: View {
    @Environment(\.theme) private var theme
    @State private var out = false
    public let color: Color
    public var size: CGFloat = 7

    public init(color: Color, size: CGFloat = 7) {
        self.color = color
        self.size = size
    }

    public var body: some View {
        Circle().fill(color).frame(width: size, height: size)
            .overlay {
                Circle().stroke(color.opacity(out ? 0 : 0.55), lineWidth: 1.5)
                    .scaleEffect(out ? 2.4 : 1)
            }
            .onChange(of: theme.motion.reduced, initial: true) { _, reduced in
                if reduced {
                    out = false
                } else {
                    withAnimation(.easeOut(duration: 1.6).repeatForever(autoreverses: false)) { out = true }
                }
            }
            .accessibilityHidden(true)
    }
}

public struct CursorBlock: View {
    @Environment(\.theme) private var theme
    @State private var dim = false
    public var width: CGFloat = 9
    public var height: CGFloat = 17
    public var blinks = true

    public init(width: CGFloat = 9, height: CGFloat = 17, blinks: Bool = true) {
        self.width = width
        self.height = height
        self.blinks = blinks
    }

    public var body: some View {
        RoundedRectangle(cornerRadius: max(1, width * 0.14), style: .continuous)
            .fill(theme.colors.accent)
            .frame(width: width, height: height)
            .opacity(dim ? 0.25 : 1)
            .onChange(of: blinks && !theme.motion.reduced, initial: true) { _, active in
                if active {
                    withAnimation(.easeInOut(duration: 0.62).repeatForever(autoreverses: true)) { dim = true }
                } else {
                    withAnimation(.easeOut(duration: 0.2)) { dim = false }
                }
            }
            .accessibilityHidden(true)
    }
}

private struct Wash<Value: Equatable>: ViewModifier {
    @Environment(\.theme) private var theme
    let trigger: Value
    let radius: CGFloat

    func body(content: Content) -> some View {
        content.background {
            if !theme.motion.reduced {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(theme.colors.glowAmber)
                    .phaseAnimator([0.0, 0.3, 0.0], trigger: trigger) { view, phase in
                        view.opacity(phase)
                    } animation: { phase in
                        phase == 0.3 ? .easeOut(duration: 0.18) : .easeOut(duration: 2.4)
                    }
                    .allowsHitTesting(false)
            }
        }
    }
}

private struct Bounce<Value: Equatable>: ViewModifier {
    @Environment(\.theme) private var theme
    let value: Value

    func body(content: Content) -> some View {
        if theme.motion.reduced {
            content
        } else {
            content.symbolEffect(.bounce, value: value)
        }
    }
}

private struct Arrive: ViewModifier {
    @Environment(\.theme) private var theme
    @State private var shown = false
    let delay: Double
    let rise: CGFloat

    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .offset(y: shown ? 0 : rise)
            .onAppear {
                guard !shown else { return }
                guard !theme.motion.reduced else { shown = true; return }
                withAnimation(.interpolatingSpring(stiffness: 260, damping: 30).delay(delay)) { shown = true }
            }
    }
}

public extension View {
    func shimmer(_ active: Bool = true) -> some View {
        modifier(Shimmer(active: active))
    }

    func wash(trigger: some Equatable, radius: CGFloat = Radius.lg) -> some View {
        modifier(Wash(trigger: trigger, radius: radius))
    }

    func arrive(delay: Double = 0, rise: CGFloat = 8) -> some View {
        modifier(Arrive(delay: delay, rise: rise))
    }

    func bounce(on value: some Equatable) -> some View {
        modifier(Bounce(value: value))
    }
}

public extension AnyTransition {
    static var rise: AnyTransition {
        .asymmetric(
            insertion: .opacity.combined(with: .offset(y: 8)).combined(with: .scale(scale: 0.97, anchor: .bottom)),
            removal: .opacity
        )
    }

    static var pop: AnyTransition {
        .scale(scale: 0.9).combined(with: .opacity)
    }
}
