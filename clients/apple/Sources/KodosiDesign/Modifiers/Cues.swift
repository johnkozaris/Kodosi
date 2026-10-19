import SwiftUI

private struct Glint<Value: Equatable>: ViewModifier {
    @Environment(\.theme) private var theme
    let trigger: Value
    let delay: Double
    let radius: CGFloat

    func body(content: Content) -> some View {
        content.overlay {
            if !theme.motion.reduced {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(theme.colors.glowAmber)
                    .keyframeAnimator(initialValue: 0.0, trigger: trigger) { view, value in
                        view.opacity(value)
                    } keyframes: { _ in
                        LinearKeyframe(0, duration: delay)
                        LinearKeyframe(0.6, duration: 0.16)
                        LinearKeyframe(0, duration: 0.4)
                    }
                    .allowsHitTesting(false)
            }
        }
    }
}

private struct RingFrame {
    var scale: CGFloat = 1
    var opacity: Double = 0
}

private struct Ring<Value: Equatable>: ViewModifier {
    @Environment(\.theme) private var theme
    let trigger: Value
    let color: Color?
    let radius: CGFloat

    func body(content: Content) -> some View {
        content.overlay {
            if !theme.motion.reduced {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(color ?? theme.colors.glowAmber, lineWidth: 2)
                    .keyframeAnimator(initialValue: RingFrame(), trigger: trigger) { view, frame in
                        view.scaleEffect(frame.scale).opacity(frame.opacity)
                    } keyframes: { _ in
                        KeyframeTrack(\.scale) {
                            LinearKeyframe(0.5, duration: 0.01)
                            CubicKeyframe(1.7, duration: 0.9)
                        }
                        KeyframeTrack(\.opacity) {
                            LinearKeyframe(1, duration: 0.01)
                            CubicKeyframe(0, duration: 0.9)
                        }
                    }
                    .allowsHitTesting(false)
            }
        }
    }
}

public struct Spinner: View {
    @Environment(\.theme) private var theme
    public var size: CGFloat = 11

    public init(size: CGFloat = 11) {
        self.size = size
    }

    public var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 60, paused: theme.motion.reduced)) { context in
            let turn = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 0.8) / 0.8
            ZStack {
                Circle().stroke(theme.colors.hairline, lineWidth: 1.5)
                Circle().trim(from: 0, to: 0.28)
                    .stroke(theme.colors.accent, style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
                    .rotationEffect(.degrees(turn * 360))
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

public extension View {
    func glint(trigger: some Equatable, delay: Double = 0, radius: CGFloat = Radius.sm) -> some View {
        modifier(Glint(trigger: trigger, delay: delay, radius: radius))
    }

    func ring(trigger: some Equatable, color: Color? = nil, radius: CGFloat = Radius.sm) -> some View {
        modifier(Ring(trigger: trigger, color: color, radius: radius))
    }
}

public extension AnyTransition {
    static var grow: AnyTransition {
        .asymmetric(
            insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: .topLeading)),
            removal: .opacity.combined(with: .scale(scale: 0.96, anchor: .top))
        )
    }
}
