import SwiftUI

struct Shimmer: ViewModifier {
    @Environment(\.theme) private var theme
    @State private var passing = false
    let active: Bool

    func body(content: Content) -> some View {
        if active, !theme.motion.reduced {
            content.overlay {
                GeometryReader { geometry in
                    let band = max(geometry.size.width * 0.5, 40)
                    LinearGradient(
                        colors: [.clear, theme.colors.glowOrange, theme.colors.glowAmber, .clear],
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

struct WorkingRim<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    @State private var turning = false
    let shape: S
    var lineWidth: CGFloat = 1.5
    var glow: CGFloat = 5

    var body: some View {
        GeometryReader { geometry in
            let side = hypot(geometry.size.width, geometry.size.height) + glow * 4
            let gradient = AngularGradient(
                colors: [
                    theme.colors.glowAmber, theme.colors.glowOrange, theme.colors.glowRose,
                    theme.colors.glowOrange.opacity(0.25), theme.colors.glowAmber,
                ],
                center: .center
            )
            .frame(width: side, height: side)
            .rotationEffect(.degrees(turning ? 360 : 0))
            .position(x: geometry.size.width / 2, y: geometry.size.height / 2)
            ZStack {
                gradient.mask(shape.strokeBorder(lineWidth: lineWidth + glow)).blur(radius: glow).opacity(0.55)
                gradient.mask(shape.strokeBorder(lineWidth: lineWidth))
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .onAppear {
            guard !theme.motion.reduced else { return }
            withAnimation(.linear(duration: 3.2).repeatForever(autoreverses: false)) { turning = true }
        }
    }
}

struct ProgressRim: View {
    @Environment(\.theme) private var theme
    let fraction: Double
    let radius: CGFloat
    var lineWidth: CGFloat = 1.5
    var glow: CGFloat = 5

    var body: some View {
        let loop = RimLoop(radius: radius, inset: lineWidth / 2)
        let done = loop.trim(from: 0, to: min(max(fraction, 0.02), 1))
        let warm = AngularGradient(
            colors: [theme.colors.glowAmber, theme.colors.glowOrange, theme.colors.glowRose],
            center: .center, startAngle: .degrees(-90), endAngle: .degrees(270)
        )
        ZStack {
            loop.stroke(theme.colors.glowOrange.opacity(0.22), lineWidth: lineWidth)
            done.stroke(warm, style: StrokeStyle(lineWidth: lineWidth + glow, lineCap: .round)).blur(radius: glow).opacity(0.5)
            done.stroke(warm, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
        }
        .animation(theme.motion.soft, value: fraction)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
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

struct BreathingDot: View {
    @Environment(\.theme) private var theme
    @State private var out = false
    let color: Color
    var size: CGFloat = 7

    var body: some View {
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

struct CursorBlock: View {
    @Environment(\.theme) private var theme
    @State private var dim = false
    var width: CGFloat = 9
    var height: CGFloat = 17
    var blinks = true

    var body: some View {
        RoundedRectangle(cornerRadius: max(1, width * 0.14), style: .continuous)
            .fill(theme.colors.accent)
            .frame(width: width, height: height)
            .shadow(color: theme.colors.accent.opacity(dim ? 0 : 0.45), radius: width * 0.6)
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

extension View {
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

extension AnyTransition {
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
