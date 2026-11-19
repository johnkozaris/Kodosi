import QuartzCore
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
                        LinearKeyframe(0.4, duration: 0.14, timingCurve: AppTheme.Motion.outUnit)
                        LinearKeyframe(0, duration: 0.5, timingCurve: AppTheme.Motion.outUnit)
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
                            LinearKeyframe(1, duration: 0.01)
                            LinearKeyframe(1.5, duration: 0.6, timingCurve: AppTheme.Motion.outUnit)
                        }
                        KeyframeTrack(\.opacity) {
                            LinearKeyframe(0.9, duration: 0.01)
                            LinearKeyframe(0, duration: 0.6, timingCurve: AppTheme.Motion.outUnit)
                        }
                    }
                    .allowsHitTesting(false)
            }
        }
    }
}

private struct SquashFrame {
    var width: CGFloat = 1
    var height: CGFloat = 1
}

private struct Squash: ViewModifier {
    @Environment(\.theme) private var theme
    let trigger: AnyHashable
    let active: Bool

    func body(content: Content) -> some View {
        if theme.motion.reduced {
            content
        } else {
            content.keyframeAnimator(initialValue: SquashFrame(), trigger: trigger) { view, frame in
                view.scaleEffect(x: active ? frame.width : 1, y: active ? frame.height : 1)
            } keyframes: { _ in
                KeyframeTrack(\.width) {
                    CubicKeyframe(1.08, duration: 0.08)
                    CubicKeyframe(0.97, duration: 0.1)
                    SpringKeyframe(1, duration: 0.3, spring: AppTheme.Motion.springShape)
                }
                KeyframeTrack(\.height) {
                    CubicKeyframe(0.9, duration: 0.08)
                    CubicKeyframe(1.04, duration: 0.1)
                    SpringKeyframe(1, duration: 0.3, spring: AppTheme.Motion.springShape)
                }
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
        LayerArt(setup: Spin(track: theme.colors.hairline, arc: theme.colors.accent, turns: !theme.motion.reduced), draw: Spin.draw)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

struct Spin: Hashable {
    static let period: TimeInterval = 0.8
    static let lineWidth: CGFloat = 1.5

    let track: Color
    let arc: Color
    let turns: Bool

    @MainActor
    static func draw(_ spin: Spin, _ canvas: LayerCanvas) {
        let bounds = CGRect(origin: .zero, size: canvas.size)
        let circle = CGPath(ellipseIn: bounds, transform: nil)
        let track = CAShapeLayer()
        track.path = circle
        track.fillColor = nil
        track.strokeColor = canvas.color(spin.track)
        track.lineWidth = lineWidth
        canvas.place(track, bounds)
        let arc = CAShapeLayer()
        arc.path = circle
        arc.fillColor = nil
        arc.strokeColor = canvas.color(spin.arc)
        arc.lineWidth = lineWidth
        arc.lineCap = .round
        arc.strokeEnd = 0.28
        canvas.place(arc, bounds)
        guard spin.turns else { return }
        let turn = CABasicAnimation(keyPath: "transform.rotation.z")
        turn.fromValue = 0
        turn.toValue = 2 * Double.pi
        turn.timingFunction = CAMediaTimingFunction(name: .linear)
        arc.add(turn.looping(period: period), forKey: "turn")
    }
}

public extension View {
    func glint(trigger: some Equatable, delay: Double = 0, radius: CGFloat = Radius.sm) -> some View {
        modifier(Glint(trigger: trigger, delay: delay, radius: radius))
    }

    func ring(trigger: some Equatable, color: Color? = nil, radius: CGFloat = Radius.sm) -> some View {
        modifier(Ring(trigger: trigger, color: color, radius: radius))
    }

    func squash(trigger: some Hashable, when active: Bool = true) -> some View {
        modifier(Squash(trigger: AnyHashable(trigger), active: active))
    }
}

public extension AnyTransition {
    static var grow: AnyTransition {
        .asymmetric(
            insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: .topLeading)),
            removal: .opacity.combined(with: .scale(scale: 0.96, anchor: .topLeading)),
        )
    }

    static var unfold: AnyTransition {
        .opacity.combined(with: .offset(y: -6))
    }
}
