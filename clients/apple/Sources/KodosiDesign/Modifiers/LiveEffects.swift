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

public struct LightRim: View {
    public enum Mode: Hashable, Sendable {
        case working
        case progress(Double)
    }

    static let tileLight = AppTheme.dark.colors.accentStrong
    static let tileCore = Color(red: 1, green: 0.96, blue: 0.92)

    @Environment(\.theme) private var theme
    public let mode: Mode
    public var cornerRadius: CGFloat?
    public var lineWidth: CGFloat
    var onTile = false

    public init(_ mode: Mode, cornerRadius: CGFloat? = nil, lineWidth: CGFloat = 2) {
        self.mode = mode
        self.cornerRadius = cornerRadius
        self.lineWidth = lineWidth
    }

    init(onTile mode: Mode, cornerRadius: CGFloat, lineWidth: CGFloat) {
        self.init(mode, cornerRadius: cornerRadius, lineWidth: lineWidth)
        onTile = true
    }

    public var body: some View {
        let light = onTile ? Self.tileLight : theme.colors.accent
        let rim = RimShape(cornerRadius: cornerRadius).inset(by: lineWidth / 2)
        ZStack {
            switch mode {
            case .working:
                if !onTile {
                    rim.stroke(light.opacity(0.2), lineWidth: lineWidth)
                }
                LayerArt(setup: TurningLight(cornerRadius: cornerRadius, lineWidth: lineWidth, light: light,
                                             core: onTile ? Self.tileCore : nil, still: theme.motion.reduced),
                         draw: TurningLight.draw)
            case let .progress(fraction):
                rim.stroke(light.opacity(0.22), lineWidth: lineWidth)
                rim.stroke(light, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round)).mask(Wedge(fraction: fraction))
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

struct RimShape: InsettableShape {
    var cornerRadius: CGFloat?
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let rect = rect.insetBy(dx: inset, dy: inset)
        let radius = min(cornerRadius.map { $0 - inset } ?? .infinity, min(rect.width, rect.height) / 2)
        return Path(roundedRect: rect, cornerRadius: max(0, radius), style: .continuous)
    }

    func inset(by amount: CGFloat) -> RimShape {
        RimShape(cornerRadius: cornerRadius, inset: inset + amount)
    }
}

struct Wedge: Shape {
    var fraction: Double

    var animatableData: Double {
        get { fraction }
        set { fraction = newValue }
    }

    func path(in rect: CGRect) -> Path {
        let center = CGPoint(x: rect.midX, y: rect.midY)
        var path = Path()
        path.move(to: center)
        path.addArc(center: center, radius: hypot(rect.width, rect.height), startAngle: .degrees(-90),
                    endAngle: .degrees(-90 + 360 * min(max(fraction, 0), 1)), clockwise: false)
        path.closeSubpath()
        return path
    }
}

struct TurningLight: Hashable {
    static let period: TimeInterval = 4
    static let rest = -CGFloat.pi / 4

    let cornerRadius: CGFloat?
    let lineWidth: CGFloat
    let light: Color
    let core: Color?
    let still: Bool

    @MainActor
    static func draw(_ art: TurningLight, _ canvas: LayerCanvas) {
        let bounds = CGRect(origin: .zero, size: canvas.size)
        let shape = RimShape(cornerRadius: art.cornerRadius)
        let side = hypot(bounds.width, bounds.height)
        if art.core != nil {
            let edge = (side - min(bounds.width, bounds.height)) / (2 * side)
            let reach = 0.42 * min(bounds.width, bounds.height) / side
            let wash = CAGradientLayer()
            wash.type = .radial
            wash.colors = [canvas.color(art.light.opacity(0.34)), canvas.color(art.light.opacity(0))]
            wash.startPoint = CGPoint(x: 0.5, y: edge)
            wash.endPoint = CGPoint(x: 0.5 + reach, y: edge + reach)
            art.turn(wash, within: shape.path(in: bounds), lineWidth: nil, on: canvas)
        }
        let arc = CAGradientLayer()
        arc.type = .conic
        arc.startPoint = CGPoint(x: 0.5, y: 0.5)
        arc.endPoint = CGPoint(x: 0.5, y: 0)
        let stops = (0 ... 36).map { Double($0) / 36 }
        arc.colors = stops.map { location in
            let glow = Self.glow(at: location)
            let color = art.core.map { art.light.mix(with: $0, by: Double(glow * glow * glow)) } ?? art.light
            return canvas.color(color.opacity(glow))
        }
        arc.locations = stops.map { NSNumber(value: $0) }
        art.turn(arc, within: shape.inset(by: art.lineWidth / 2).path(in: bounds), lineWidth: art.lineWidth, on: canvas)
    }

    static func glow(at location: Double) -> CGFloat {
        let ahead = location <= 0.5
        let degrees = (ahead ? location : 1 - location) * 360
        let spread: Double = ahead ? 20 : 36
        return CGFloat(exp(-(degrees * degrees) / (2 * spread * spread)))
    }

    @MainActor
    private func turn(_ gradient: CAGradientLayer, within path: Path, lineWidth: CGFloat?, on canvas: LayerCanvas) {
        let bounds = CGRect(origin: .zero, size: canvas.size)
        let mask = CAShapeLayer()
        mask.frame = bounds
        mask.contentsScale = canvas.scale
        mask.path = path.cgPath
        if let lineWidth {
            mask.fillColor = nil
            mask.strokeColor = CGColor(gray: 0, alpha: 1)
            mask.lineWidth = lineWidth
        } else {
            mask.fillColor = CGColor(gray: 0, alpha: 1)
        }
        let side = hypot(bounds.width, bounds.height)
        gradient.frame = CGRect(x: bounds.midX - side / 2, y: bounds.midY - side / 2, width: side, height: side)
        gradient.contentsScale = canvas.scale
        let holder = CALayer()
        holder.mask = mask
        holder.addSublayer(gradient)
        canvas.place(holder, bounds)
        if still {
            gradient.setAffineTransform(CGAffineTransform(rotationAngle: Self.rest))
            return
        }
        let spin = CABasicAnimation(keyPath: "transform.rotation.z")
        spin.fromValue = 0
        spin.toValue = 2 * Double.pi
        spin.timingFunction = CAMediaTimingFunction(name: .linear)
        gradient.add(spin.looping(period: Self.period), forKey: "turn")
    }
}

struct LightLine: View {
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
        Capsule()
            .fill(mode == .done ? theme.colors.accent : theme.colors.accentSoft)
            .overlay {
                if mode == .flowing {
                    LayerArt(setup: GlidingLight(light: theme.colors.accent, still: theme.motion.reduced), draw: GlidingLight.draw)
                        .clipShape(Capsule())
                }
            }
            .frame(height: lineWidth)
            .animation(theme.motion.fade, value: mode)
    }
}

struct GlidingLight: Hashable {
    static let period: TimeInterval = 2.4

    let light: Color
    let still: Bool

    @MainActor
    static func draw(_ art: GlidingLight, _ canvas: LayerCanvas) {
        let light = canvas.color(art.light)
        let band = CAGradientLayer()
        band.startPoint = CGPoint(x: 0, y: 0.5)
        band.endPoint = CGPoint(x: 1, y: 0.5)
        band.colors = [light.copy(alpha: 0), light, light.copy(alpha: 0)].compactMap(\.self)
        let width = max(canvas.size.height * 8, canvas.size.width * 0.45)
        canvas.place(band, CGRect(x: 0, y: 0, width: width, height: canvas.size.height))
        if art.still {
            band.position.x = canvas.size.width / 2
            return
        }
        let glide = CABasicAnimation(keyPath: "position.x")
        glide.fromValue = -width / 2
        glide.toValue = canvas.size.width + width / 2
        glide.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        band.add(glide.looping(period: period), forKey: "glide")
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
