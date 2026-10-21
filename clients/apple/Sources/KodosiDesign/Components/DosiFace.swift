import KodosiBridge
import QuartzCore
import SwiftUI
#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

public struct DosiFace: View {
    public enum Mood: Hashable, Sendable {
        case resting, listening, thinking, reading, working, talking, asking, stuck, stopping, off

        public var label: String {
            switch self {
            case .resting: String(localized: "Ready", bundle: .module)
            case .listening: String(localized: "Listening", bundle: .module)
            case .thinking: String(localized: "Thinking", bundle: .module)
            case .reading: String(localized: "Reading", bundle: .module)
            case .working: String(localized: "Working", bundle: .module)
            case .talking: String(localized: "Writing", bundle: .module)
            case .asking: String(localized: "Asking you", bundle: .module)
            case .stuck: String(localized: "Stuck", bundle: .module)
            case .stopping: String(localized: "Stopping", bundle: .module)
            case .off: String(localized: "Off", bundle: .module)
            }
        }
    }

    @Environment(\.theme) private var theme
    public let mood: Mood
    public var size: CGFloat = 22
    public var still = false
    public var look = CGPoint.zero
    public var reacts = 0

    public init(mood: Mood, size: CGFloat = 22, still: Bool = false, look: CGPoint = .zero, reacts: Int = 0) {
        self.mood = mood
        self.size = size
        self.still = still
        self.look = look
        self.reacts = reacts
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
        ZStack {
            shape.fill(theme.colors.terminal)
            shape.strokeBorder(
                LinearGradient(colors: [.white.opacity(0.18), .clear, .black.opacity(0.3)], startPoint: .topLeading, endPoint: .bottomTrailing),
                lineWidth: max(0.5, size * 0.03),
            )
            FaceHost(art: FaceArt(mood: mood, loops: !still && !theme.motion.reduced, eases: !theme.motion.reduced,
                                  copper: AppTheme.dark.colors.accent),
                     look: still ? .zero : look, reacts: still ? 0 : reacts)
                .allowsHitTesting(false)
        }
        .frame(width: size, height: size)
        .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.2), radius: size * 0.1, x: size * 0.03, y: size * 0.07)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: "Dosi"))
        .accessibilityValue(Text(verbatim: mood.label))
    }
}

struct FaceArt: Equatable {
    let mood: DosiFace.Mood
    let loops: Bool
    let eases: Bool
    let copper: Color
}

private struct FacePose {
    var gaze = CGPoint.zero
    var left: CGFloat = 1
    var right: CGFloat = 1
    var tilt: CGFloat = 0
    var opacity: Float = 1

    init(_ mood: DosiFace.Mood, closes: Bool) {
        switch mood {
        case .resting, .talking: break
        case .listening: (gaze, left, right) = (CGPoint(x: 0, y: 0.05), 1.08, 1.08)
        case .thinking: (gaze, left) = (CGPoint(x: 0.06, y: -0.06), 0.75)
        case .reading: gaze = CGPoint(x: 0.07, y: 0.01)
        case .working: (gaze, left, right) = (CGPoint(x: 0.05, y: 0.06), 0.8, 0.8)
        case .asking: (gaze, left, right, tilt) = (CGPoint(x: 0, y: -0.02), 1.12, 1.12, 0.14)
        case .stuck: (gaze, left, right, opacity) = (CGPoint(x: 0, y: 0.04), 0.22, 0.22, 0.75)
        case .stopping: (left, right) = closes ? (0.2, 0.2) : (1, 1)
        case .off: (left, right, opacity) = (0.18, 0.18, 0.35)
        }
    }
}

private struct FaceSurface: Equatable {
    let size: CGSize
    let scale: CGFloat
    let live: Bool
}

@MainActor
private final class FaceRig {
    private struct State: Equatable {
        let art: FaceArt
        let surface: FaceSurface
    }

    private struct Frame {
        let pose: CATransform3D
        let opacity: Float
        let drift: CATransform3D
        let eyes: [CATransform3D]
        let shades: [Float]
    }

    let root = CALayer()
    private let pose = CALayer()
    private let gaze = CALayer()
    private let drift = CALayer()
    private let eyes = [CALayer(), CALayer()]
    private var state: State?
    private var looking = CGPoint.zero
    private var reacted: Int?

    init() {
        root.addSublayer(pose)
        pose.addSublayer(gaze)
        gaze.addSublayer(drift)
        eyes.forEach(drift.addSublayer)
    }

    func show(_ art: FaceArt, look: CGPoint, reacts: Int, on surface: FaceSurface, color: CGColor) {
        let size = surface.size
        let live = surface.live
        guard size.width > 0, size.height > 0 else { return }
        let next = State(art: art, surface: surface)
        let side = min(size.width, size.height)
        let previous = state
        let relaid = previous?.surface.size != size || previous?.surface.scale != surface.scale
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if next != previous {
            state = next
            if relaid {
                layout(size, scale: surface.scale)
            }
            eyes.forEach { $0.backgroundColor = color }
            let from = live && art.eases && previous?.surface.live == true && !relaid ? frame() : nil
            stopLoops()
            apply(FacePose(art.mood, closes: art.eases), side: side)
            if let from {
                ease(from, closing: art.mood == .stopping && previous?.art.mood != .stopping)
            }
            if live, art.loops {
                startLoops(art.mood, side: side)
            }
        }
        if look != looking || relaid {
            glance(look, side: side, eases: live && art.eases && !relaid)
        }
        if let reacted, reacts != reacted, live, art.eases {
            react()
        }
        reacted = reacts
        CATransaction.commit()
    }

    private func glance(_ point: CGPoint, side: CGFloat, eases: Bool) {
        let from = (gaze.presentation() ?? gaze).transform
        gaze.transform = CATransform3DMakeTranslation(point.x * side * 0.09, point.y * side * 0.07, 0)
        looking = point
        if eases {
            Self.settle(gaze, "transform", NSValue(caTransform3D: from), NSValue(caTransform3D: gaze.transform),
                        shape: AppTheme.Motion.softShape)
        }
    }

    private func react() {
        let blink = CAKeyframeAnimation(keyPath: "transform.scale.y")
        blink.values = [1, 0.15, 1.08, 1]
        blink.keyTimes = [0, 0.3, 0.7, 1]
        blink.duration = 0.36
        let curve = AppTheme.Motion.outCurve
        let out = CAMediaTimingFunction(controlPoints: curve[0], curve[1], curve[2], curve[3])
        blink.timingFunctions = [CAMediaTimingFunction(name: .linear), out, out]
        drift.add(blink, forKey: "react.blink")
    }

    private func layout(_ size: CGSize, scale: CGFloat) {
        let bounds = CGRect(origin: .zero, size: size)
        for layer in [root, pose, gaze, drift] {
            layer.bounds = bounds
            layer.position = CGPoint(x: size.width / 2, y: size.height / 2)
            layer.contentsScale = scale
        }
        let side = min(size.width, size.height)
        let width = side * 0.15
        let height = side * 0.3
        let gap = side * 0.12
        for (index, eye) in eyes.enumerated() {
            let direction: CGFloat = index == 0 ? -1 : 1
            eye.bounds = CGRect(x: 0, y: 0, width: width, height: height)
            eye.position = CGPoint(x: size.width / 2 + direction * (gap / 2 + width / 2), y: size.height / 2)
            eye.cornerRadius = width * 0.3
            eye.cornerCurve = .continuous
            eye.contentsScale = scale
        }
    }

    private func apply(_ look: FacePose, side: CGFloat) {
        pose.transform = CATransform3DRotate(CATransform3DMakeTranslation(look.gaze.x * side, look.gaze.y * side, 0), look.tilt, 0, 0, 1)
        pose.opacity = look.opacity
        drift.transform = CATransform3DIdentity
        eyes[0].transform = CATransform3DMakeScale(1, look.left, 1)
        eyes[1].transform = CATransform3DMakeScale(1, look.right, 1)
        eyes.forEach { $0.opacity = 1 }
    }

    private func frame() -> Frame {
        let shown = pose.presentation() ?? pose
        return Frame(
            pose: shown.transform, opacity: shown.opacity, drift: (drift.presentation() ?? drift).transform,
            eyes: eyes.map { ($0.presentation() ?? $0).transform }, shades: eyes.map { ($0.presentation() ?? $0).opacity },
        )
    }

    private func ease(_ from: Frame, closing: Bool) {
        Self.settle(pose, "transform", NSValue(caTransform3D: from.pose), NSValue(caTransform3D: pose.transform))
        Self.settle(pose, "opacity", NSNumber(value: from.opacity), NSNumber(value: pose.opacity))
        Self.settle(drift, "transform", NSValue(caTransform3D: from.drift), NSValue(caTransform3D: drift.transform))
        for (index, eye) in eyes.enumerated() {
            let start = NSValue(caTransform3D: from.eyes[index])
            let end = NSValue(caTransform3D: eye.transform)
            if closing {
                let close = CABasicAnimation(keyPath: "transform")
                close.fromValue = start
                close.toValue = end
                close.duration = 0.35
                let curve = AppTheme.Motion.outCurve
                close.timingFunction = CAMediaTimingFunction(controlPoints: curve[0], curve[1], curve[2], curve[3])
                eye.add(close, forKey: "pose.transform")
            } else {
                Self.settle(eye, "transform", start, end)
            }
            Self.settle(eye, "opacity", NSNumber(value: from.shades[index]), NSNumber(value: eye.opacity))
        }
    }

    private static func settle(_ layer: CALayer, _ key: String, _ from: Any, _ to: Any, shape: Spring = AppTheme.Motion.snappyShape) {
        let spring = CASpringAnimation(keyPath: key)
        spring.mass = CGFloat(shape.mass)
        spring.stiffness = CGFloat(shape.stiffness)
        spring.damping = CGFloat(shape.damping)
        spring.fromValue = from
        spring.toValue = to
        spring.duration = spring.settlingDuration
        layer.add(spring, forKey: "pose.\(key)")
    }

    private func stopLoops() {
        for layer in [drift] + eyes {
            for key in layer.animationKeys() ?? [] where key.hasPrefix("loop.") {
                layer.removeAnimation(forKey: key)
            }
        }
    }

    private func startLoops(_ mood: DosiFace.Mood, side: CGFloat) {
        switch mood {
        case .talking:
            drift.add(Self.wander("transform.translation.y", [0, 0.02 * side, 0], period: 1.6), forKey: "loop.nod")
        case .thinking:
            drift.add(Self.wander("transform.translation.x", [0, -0.035 * side, 0.02 * side, 0], period: 3.6), forKey: "loop.x")
            drift.add(Self.wander("transform.translation.y", [0, -0.025 * side, 0.015 * side, 0], period: 3.6), forKey: "loop.y")
            eyes[0].add(Self.wander("transform.scale.y", [0, 0.18, 0], period: 3.6), forKey: "loop.ponder")
        case .reading:
            drift.add(Self.wander("transform.translation.x", [0, 0.07 * side, 0], period: 3.2), forKey: "loop.scan")
            drift.add(Self.wander("transform.translation.y", [0, 0.02 * side, 0], period: 6.4), forKey: "loop.lines")
        case .working:
            drift.add(Self.wander("transform.translation.y", [0, 0.025 * side, 0], period: 2.4), forKey: "loop.y")
            drift.add(Self.wander("transform.translation.x", [0, 0.03 * side, 0, -0.03 * side, 0], period: 4.8), forKey: "loop.x")
        case .resting, .listening, .asking, .stuck, .stopping, .off:
            break
        }
    }

    private static func wander(_ keyPath: String, _ values: [CGFloat], period: TimeInterval) -> CAKeyframeAnimation {
        let wander = CAKeyframeAnimation(keyPath: keyPath)
        wander.values = values
        wander.isAdditive = true
        wander.calculationMode = .cubic
        return repeating(wander, period: period)
    }

    private static func repeating(_ animation: CAKeyframeAnimation, period: TimeInterval) -> CAKeyframeAnimation {
        animation.duration = period
        animation.repeatCount = .infinity
        animation.isRemovedOnCompletion = false
        return animation
    }
}

#if os(macOS)
    private struct FaceHost: NSViewRepresentable {
        let art: FaceArt
        let look: CGPoint
        let reacts: Int

        func makeNSView(context _: Context) -> FaceView {
            FaceView()
        }

        func updateNSView(_ view: FaceView, context: Context) {
            view.show(art, look: look, reacts: reacts, color: art.copper.resolve(in: context.environment).cgColor)
        }
    }

    private final class FaceView: NSView {
        private let rig = FaceRig()
        private var art: FaceArt?
        private var look = CGPoint.zero
        private var reacts = 0
        private var color: CGColor?

        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
            layer?.addSublayer(rig.root)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not available")
        }

        override var isFlipped: Bool {
            true
        }

        override func hitTest(_: NSPoint) -> NSView? {
            nil
        }

        func show(_ art: FaceArt, look: CGPoint, reacts: Int, color: CGColor) {
            self.art = art
            self.look = look
            self.reacts = reacts
            self.color = color
            refresh()
        }

        override func layout() {
            super.layout()
            refresh()
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            refresh()
        }

        override func viewDidChangeBackingProperties() {
            super.viewDidChangeBackingProperties()
            refresh()
        }

        private func refresh() {
            guard let art, let color else { return }
            let surface = FaceSurface(size: bounds.size, scale: window?.backingScaleFactor ?? 2, live: window != nil)
            rig.show(art, look: look, reacts: reacts, on: surface, color: color)
        }
    }
#else
    private struct FaceHost: UIViewRepresentable {
        let art: FaceArt
        let look: CGPoint
        let reacts: Int

        func makeUIView(context _: Context) -> FaceView {
            FaceView()
        }

        func updateUIView(_ view: FaceView, context: Context) {
            view.show(art, look: look, reacts: reacts, color: art.copper.resolve(in: context.environment).cgColor)
        }
    }

    private final class FaceView: UIView {
        private let rig = FaceRig()
        private var art: FaceArt?
        private var look = CGPoint.zero
        private var reacts = 0
        private var color: CGColor?

        override init(frame: CGRect) {
            super.init(frame: frame)
            isUserInteractionEnabled = false
            layer.addSublayer(rig.root)
            registerForTraitChanges([UITraitDisplayScale.self]) { (view: FaceView, _: UITraitCollection) in
                view.refresh()
            }
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not available")
        }

        func show(_ art: FaceArt, look: CGPoint, reacts: Int, color: CGColor) {
            self.art = art
            self.look = look
            self.reacts = reacts
            self.color = color
            refresh()
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            refresh()
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            refresh()
        }

        private func refresh() {
            guard let art, let color else { return }
            let surface = FaceSurface(size: bounds.size, scale: traitCollection.displayScale, live: window != nil)
            rig.show(art, look: look, reacts: reacts, on: surface, color: color)
        }
    }
#endif

public extension DosiStore {
    func mood(listening: Bool = false) -> DosiFace.Mood {
        switch state {
        case .unavailable: .off
        case .stopping: .stopping
        case .connecting: .resting
        case .idle: idleMood(listening: listening)
        case .working: workingMood
        }
    }

    private func idleMood(listening: Bool) -> DosiFace.Mood {
        if !openQuestions.isEmpty {
            return .asking
        }
        if case let .notice(notice)? = items.last?.body, notice.tone != .stopped {
            return .stuck
        }
        return listening ? .listening : .resting
    }

    private var workingMood: DosiFace.Mood {
        switch items.last?.body {
        case .reasoning(_, true, _)?: .thinking
        case .text(_, true)?: .talking
        case .question?: .asking
        case let .tool(tool)? where tool.status == .running:
            [.look, .read, .wait, .search, .fetch].contains(tool.action) ? .reading : .working
        default: .thinking
        }
    }
}
