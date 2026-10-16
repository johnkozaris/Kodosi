import QuartzCore
import SwiftUI

private struct Shimmer: ViewModifier {
    @Environment(\.theme) private var theme
    let active: Bool

    func body(content: Content) -> some View {
        if active, !theme.motion.reduced {
            content.overlay {
                TimelineView(.animation(minimumInterval: 1 / 30)) { context in
                    GeometryReader { geometry in
                        let band = max(geometry.size.width * 0.5, 40)
                        let pass = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.9) / 1.9
                        LinearGradient(colors: [.clear, theme.colors.ink, .clear], startPoint: .leading, endPoint: .trailing)
                            .frame(width: band)
                            .offset(x: -band + (geometry.size.width + band) * pass * pass * (3 - 2 * pass))
                    }
                }
                .mask(content)
                .allowsHitTesting(false)
            }
        } else {
            content
        }
    }
}

public struct CellRim: View {
    public enum Mode: Hashable, Sendable {
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
        let margin = (lineWidth * 1.5).rounded(.up)
        LayerArt(setup: CellTrackArt(kind: .rim(mode, cornerRadius: cornerRadius), lineWidth: lineWidth, margin: margin,
                                     colors: theme.colors, still: theme.motion.reduced), draw: CellTrackArt.draw)
            .padding(-margin)
    }
}

struct CellLine: View {
    enum Mode: Hashable, Sendable {
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
        LayerArt(setup: CellTrackArt(kind: .line(mode), lineWidth: lineWidth, margin: 0, colors: theme.colors,
                                     still: theme.motion.reduced), draw: CellTrackArt.draw)
            .frame(height: lineWidth * 3)
    }
}

struct CellTrackArt: Hashable {
    enum Kind: Hashable {
        case rim(CellRim.Mode, cornerRadius: CGFloat?)
        case line(CellLine.Mode)
    }

    let kind: Kind
    let lineWidth: CGFloat
    let margin: CGFloat
    let strong: Color
    let accent: Color
    let soft: Color
    let still: Bool

    init(kind: Kind, lineWidth: CGFloat, margin: CGFloat, colors: AppTheme.Colors, still: Bool) {
        self.kind = kind
        self.lineWidth = lineWidth
        self.margin = margin
        strong = colors.accentStrong
        accent = colors.accent
        soft = colors.accentSoft
        self.still = still
    }

    @MainActor
    static func draw(_ art: CellTrackArt, _ canvas: LayerCanvas) {
        let rect = CGRect(origin: .zero, size: canvas.size).insetBy(dx: art.margin, dy: art.margin)
        let painter = CellPainter(canvas: canvas, art: art)
        switch art.kind {
        case let .rim(mode, cornerRadius):
            guard let shape = CellTrackShape.rim(in: rect, cornerRadius: cornerRadius, thickness: art.lineWidth, scale: canvas.scale)
            else { return }
            painter.rim(shape, mode: mode)
        case let .line(mode):
            guard let shape = CellTrackShape.line(in: rect, thickness: art.lineWidth, scale: canvas.scale) else { return }
            painter.line(shape, mode: mode)
        }
    }
}

@MainActor
private final class CellPainter {
    private let canvas: LayerCanvas
    private let art: CellTrackArt
    private var colors: [CellInk: CGColor] = [:]

    init(canvas: LayerCanvas, art: CellTrackArt) {
        self.canvas = canvas
        self.art = art
    }

    func rim(_ shape: CellTrackShape, mode: CellRim.Mode) {
        let count = shape.cells.count
        switch mode {
        case .working where art.still:
            paint(shape, CellTrackFrames.stillRim(count: count), cursor: 0, blinks: false)
        case .working:
            animate(shape, .rim(count: count))
        case let .progress(fraction):
            let cursor = fraction < 1 ? min(count - 1, Int(fraction * Double(count))) : nil
            paint(shape, CellTrackFrames.progress(count: count, fraction: fraction), cursor: cursor, blinks: !art.still)
        }
    }

    func line(_ shape: CellTrackShape, mode: CellLine.Mode) {
        let count = shape.cells.count
        switch mode {
        case .waiting:
            paint(shape, Array(repeating: .mix(1), count: count), cursor: nil, blinks: false)
        case .done:
            paint(shape, Array(repeating: .mix(0), count: count), cursor: nil, blinks: false)
        case .flowing where art.still:
            let frames = CellTrackFrames.line(count: count)
            paint(shape, frames.inks(at: 0), cursor: frames.cursor(at: 0), blinks: false)
        case .flowing:
            animate(shape, .line(count: count))
        }
    }

    private func color(_ ink: CellInk) -> CGColor {
        if let known = colors[ink] {
            return known
        }
        let resolved = switch ink {
        case .strong: canvas.color(art.strong)
        case .mix(0): canvas.color(art.accent)
        case .mix(1): canvas.color(art.soft)
        case let .mix(amount): canvas.color(art.accent.mix(with: art.soft, by: amount))
        }
        colors[ink] = resolved
        return resolved
    }

    private func paint(_ shape: CellTrackShape, _ inks: [CellInk], cursor: Int?, blinks: Bool) {
        for (index, frame) in shape.cells.enumerated() {
            let cell = CALayer()
            cell.backgroundColor = color(inks[index])
            canvas.place(cell, frame, radius: shape.cellRadius)
        }
        guard let cursor else { return }
        let block = CALayer()
        block.backgroundColor = color(.strong)
        canvas.place(block, shape.cursors[cursor], radius: shape.cursorRadius)
        if blinks {
            block.add(CAKeyframeAnimation.steps("opacity", [(0, Float(1)), (0.53, Float(0))], period: 1.06), forKey: "blink")
        }
    }

    private func animate(_ shape: CellTrackShape, _ frames: CellTrackFrames) {
        let phase = loopPhase(frames.period)
        let inks = frames.inks(at: phase)
        for (index, frame) in shape.cells.enumerated() {
            let cell = CALayer()
            cell.backgroundColor = color(inks[index])
            canvas.place(cell, frame, radius: shape.cellRadius)
            let keys = frames.cells[index]
            if keys.count > 1 {
                cell.add(CAKeyframeAnimation.steps("backgroundColor", keys.map { ($0.time, color($0.value)) }, period: frames.period),
                         forKey: "type")
            }
        }
        var cell = frames.cursor.last { $0.value != nil }?.value ?? 0
        let block = CALayer()
        block.backgroundColor = color(.strong)
        canvas.place(block, shape.cursors[frames.cursor(at: phase) ?? cell], radius: shape.cursorRadius)
        block.opacity = frames.cursor(at: phase) == nil ? 0 : 1
        let places = frames.cursor.map { key -> (time: TimeInterval, value: Any) in
            cell = key.value ?? cell
            let frame = shape.cursors[cell]
            return (key.time, Self.pointValue(CGPoint(x: frame.midX, y: frame.midY)))
        }
        block.add(CAKeyframeAnimation.steps("position", places, period: frames.period), forKey: "type")
        block.add(CAKeyframeAnimation.steps("opacity", frames.cursor.map { ($0.time, Float($0.value == nil ? 0 : 1)) }, period: frames.period),
                  forKey: "blink")
    }

    private static func pointValue(_ point: CGPoint) -> NSValue {
        #if os(macOS)
            NSValue(point: point)
        #else
            NSValue(cgPoint: point)
        #endif
    }
}

public struct BreathingDot: View {
    @Environment(\.theme) private var theme
    public let color: Color
    public var size: CGFloat = 7

    public init(color: Color, size: CGFloat = 7) {
        self.color = color
        self.size = size
    }

    public var body: some View {
        Circle().fill(color).frame(width: size, height: size)
            .overlay {
                if !theme.motion.reduced {
                    LayerArt(setup: Breath(color: color, size: size), draw: Breath.draw)
                        .frame(width: size * 2.6, height: size * 2.6)
                }
            }
            .accessibilityHidden(true)
    }
}

struct Breath: Hashable {
    let color: Color
    let size: CGFloat

    @MainActor
    static func draw(_ breath: Breath, _ canvas: LayerCanvas) {
        let ring = CAShapeLayer()
        let bounds = CGRect(x: 0, y: 0, width: breath.size, height: breath.size)
        ring.path = CGPath(ellipseIn: bounds, transform: nil)
        ring.fillColor = nil
        ring.strokeColor = canvas.color(breath.color)
        ring.lineWidth = 1.5
        ring.opacity = 0
        canvas.place(ring, bounds.offsetBy(dx: (canvas.size.width - breath.size) / 2, dy: (canvas.size.height - breath.size) / 2))
        let grow = CABasicAnimation(keyPath: "transform.scale")
        grow.fromValue = 1
        grow.toValue = 2.4
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = 0.55
        fade.toValue = 0
        let group = CAAnimationGroup()
        group.animations = [grow, fade]
        group.timingFunction = CAMediaTimingFunction(name: .easeOut)
        ring.add(group.looping(period: 1.6), forKey: "breath")
    }
}

public struct CursorBlock: View {
    public enum Blink: Hashable, Sendable {
        case steady, fading, typing
    }

    @Environment(\.theme) private var theme
    public var width: CGFloat = 9
    public var height: CGFloat = 17
    public var blink: Blink = .fading

    public init(width: CGFloat = 9, height: CGFloat = 17, blinks: Bool = true) {
        self.init(width: width, height: height, blink: blinks ? .fading : .steady)
    }

    public init(width: CGFloat, height: CGFloat, blink: Blink) {
        self.width = width
        self.height = height
        self.blink = blink
    }

    public var body: some View {
        let radius = max(1, width * 0.14)
        Group {
            if blink == .steady || theme.motion.reduced {
                RoundedRectangle(cornerRadius: radius, style: .continuous).fill(theme.colors.accent)
            } else {
                LayerArt(setup: Cursor(color: theme.colors.accent, radius: radius, blink: blink), draw: Cursor.draw)
            }
        }
        .frame(width: width, height: height)
        .accessibilityHidden(true)
    }
}

struct Cursor: Hashable {
    let color: Color
    let radius: CGFloat
    let blink: CursorBlock.Blink

    @MainActor
    static func draw(_ cursor: Cursor, _ canvas: LayerCanvas) {
        let block = CALayer()
        block.backgroundColor = canvas.color(cursor.color)
        canvas.place(block, CGRect(origin: .zero, size: canvas.size), radius: cursor.radius)
        switch cursor.blink {
        case .steady:
            break
        case .fading:
            let fade = CABasicAnimation(keyPath: "opacity")
            fade.fromValue = 1
            fade.toValue = 0.25
            fade.autoreverses = true
            fade.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            let loop = fade.looping(period: 0.62)
            loop.timeOffset = loopPhase(1.24)
            block.add(loop, forKey: "blink")
        case .typing:
            let blink = CAKeyframeAnimation(keyPath: "opacity")
            blink.values = [1, 0, 0, 1, 1].map { Float($0) }
            blink.keyTimes = [0, 0.08, 0.44, 0.52, 1]
            blink.beginTime = block.convertTime(CACurrentMediaTime(), from: nil) + 1.1
            blink.duration = 1
            blink.repeatCount = .infinity
            blink.isRemovedOnCompletion = false
            block.add(blink, forKey: "blink")
        }
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
