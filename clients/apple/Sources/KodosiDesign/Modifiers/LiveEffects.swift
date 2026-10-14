import SwiftUI

public struct Shimmer: ViewModifier {
    @Environment(\.theme) private var theme
    @State private var passing = false
    public let active: Bool

    public init(active: Bool) {
        self.active = active
    }

    public func body(content: Content) -> some View {
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

/// A copper arc that runs round `shape` while something works: solid, with a
/// muted track under it, and a length that breathes as it turns.
public struct WorkingRim<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    public let shape: S
    public var lineWidth: CGFloat = 1.5

    public init(shape: S, lineWidth: CGFloat = 1.5) {
        self.shape = shape
        self.lineWidth = lineWidth
    }

    public var body: some View {
        let path = shape.inset(by: lineWidth / 2)
        TimelineView(.animation(paused: theme.motion.reduced)) { context in
            let turn = theme.motion.reduced ? 0 : context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 2.4) / 2.4
            let length = 0.24 + 0.08 * sin(turn * 2 * .pi)
            ZStack {
                path.stroke(theme.colors.accentSoft, lineWidth: lineWidth)
                RimArc(shape: path, from: turn, length: length)
                    .stroke(theme.colors.accent, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// How far a piece of work is, as a solid copper arc on a muted track.
public struct ProgressRim: View {
    @Environment(\.theme) private var theme
    public let fraction: Double
    public let radius: CGFloat
    public var lineWidth: CGFloat = 1.5

    public init(fraction: Double, radius: CGFloat, lineWidth: CGFloat = 1.5) {
        self.fraction = fraction
        self.radius = radius
        self.lineWidth = lineWidth
    }

    public var body: some View {
        let loop = RimLoop(radius: radius, inset: lineWidth / 2)
        ZStack {
            loop.stroke(theme.colors.accentSoft, lineWidth: lineWidth)
            loop.trim(from: 0, to: min(max(fraction, 0.02), 1))
                .stroke(theme.colors.accent, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
        }
        .animation(theme.motion.soft, value: fraction)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// A piece of `shape`'s outline that starts at `from` and may run past its end
/// back to the start.
private struct RimArc<S: Shape>: Shape {
    let shape: S
    let from: CGFloat
    let length: CGFloat

    func path(in rect: CGRect) -> Path {
        let outline = shape.path(in: rect)
        let end = from + length
        var path = outline.trimmedPath(from: from, to: min(end, 1))
        if end > 1 {
            path.addPath(outline.trimmedPath(from: 0, to: end - 1))
        }
        return path
    }
}

private struct RimLoop: Shape {
    let radius: CGFloat
    let inset: CGFloat

    func path(in rect: CGRect) -> Path {
        let rect = rect.insetBy(dx: inset, dy: inset)
        let radius = min(radius - inset, min(rect.width, rect.height) / 2)
        var path = Path()
        path.move(to: CGPoint(x: rect.midX, y: rect.minY))
        for (corner, next) in [
            (CGPoint(x: rect.maxX, y: rect.minY), CGPoint(x: rect.maxX, y: rect.maxY)),
            (CGPoint(x: rect.maxX, y: rect.maxY), CGPoint(x: rect.minX, y: rect.maxY)),
            (CGPoint(x: rect.minX, y: rect.maxY), CGPoint(x: rect.minX, y: rect.minY)),
            (CGPoint(x: rect.minX, y: rect.minY), CGPoint(x: rect.maxX, y: rect.minY)),
        ] {
            path.addArc(tangent1End: corner, tangent2End: next, radius: radius)
        }
        path.closeSubpath()
        return path
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
            .onAppear {
                guard !theme.motion.reduced else { return }
                withAnimation(.easeOut(duration: 1.6).repeatForever(autoreverses: false)) { out = true }
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
