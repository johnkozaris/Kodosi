import KodosiBridge
import QuartzCore
import SwiftUI

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

    public init(mood: Mood, size: CGFloat = 22, still: Bool = false) {
        self.mood = mood
        self.size = size
        self.still = still
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
        ZStack {
            shape.fill(theme.colors.terminal)
            shape.strokeBorder(
                LinearGradient(colors: [.white.opacity(0.18), .clear, .black.opacity(0.3)], startPoint: .topLeading, endPoint: .bottomTrailing),
                lineWidth: max(0.5, size * 0.03)
            )
            LayerArt(setup: FaceArt(mood: mood, still: still || theme.motion.reduced, copper: AppTheme.dark.colors.accent),
                     draw: FaceArt.draw)
                .id(mood)
                .transition(.opacity)
        }
        .frame(width: size, height: size)
        .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.2), radius: size * 0.1, x: size * 0.03, y: size * 0.07)
        .animation(theme.motion.fade, value: mood)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: "Dosi"))
        .accessibilityValue(Text(verbatim: mood.label))
    }
}

struct FaceArt: Hashable {
    let mood: DosiFace.Mood
    let still: Bool
    let copper: Color

    private struct Pose {
        var gaze = CGPoint.zero
        var left: CGFloat = 1
        var right: CGFloat = 1
        var tilt: CGFloat = 0
        var opacity: Float = 1
    }

    private var pose: Pose {
        switch mood {
        case .resting: Pose()
        case .listening: Pose(gaze: CGPoint(x: 0, y: 0.05), left: 1.08, right: 1.08)
        case .thinking: Pose(gaze: CGPoint(x: 0.06, y: -0.06), left: 0.75)
        case .reading: Pose(gaze: CGPoint(x: -0.07, y: -0.03))
        case .working: Pose(gaze: CGPoint(x: 0, y: 0.06), left: 0.8, right: 0.8)
        case .talking: Pose()
        case .asking: Pose(gaze: CGPoint(x: 0, y: -0.02), left: 1.12, right: 1.12, tilt: 0.14)
        case .stuck: Pose(gaze: CGPoint(x: 0, y: 0.04), left: 0.22, right: 0.22, opacity: 0.75)
        case .stopping: Pose()
        case .off: Pose(left: 0.18, right: 0.18, opacity: 0.35)
        }
    }

    @MainActor
    static func draw(_ art: FaceArt, _ canvas: LayerCanvas) {
        let side = min(canvas.size.width, canvas.size.height)
        let width = side * 0.15
        let height = side * 0.3
        let gap = side * 0.12
        let pose = art.pose
        let gaze = CALayer()
        canvas.place(gaze, CGRect(origin: .zero, size: canvas.size))
        gaze.opacity = pose.opacity
        var transform = CATransform3DMakeTranslation(pose.gaze.x * side, pose.gaze.y * side, 0)
        transform = CATransform3DRotate(transform, pose.tilt, 0, 0, 1)
        gaze.transform = transform
        let eyes = [(CGFloat(-1), pose.left), (CGFloat(1), pose.right)].map { direction, scale in
            let eye = CALayer()
            eye.backgroundColor = canvas.color(art.copper)
            let x = canvas.size.width / 2 + direction * (gap / 2 + width / 2) - width / 2
            eye.frame = CGRect(x: x, y: (canvas.size.height - height) / 2, width: width, height: height)
            eye.cornerRadius = width * 0.3
            eye.cornerCurve = .continuous
            eye.contentsScale = canvas.scale
            eye.transform = CATransform3DMakeScale(1, scale, 1)
            gaze.addSublayer(eye)
            return eye
        }
        guard !art.still else { return }
        art.animate(gaze: gaze, eyes: eyes, side: side)
    }

    private var blinkPeriod: TimeInterval? {
        switch mood {
        case .resting, .asking: 4.2
        case .listening: 3.2
        case .talking: 3.6
        default: nil
        }
    }

    @MainActor
    private func animate(gaze: CALayer, eyes: [CALayer], side: CGFloat) {
        if let blinkPeriod {
            eyes.forEach { $0.add(Self.blink(period: blinkPeriod), forKey: "blink") }
        }
        switch mood {
        case .talking: gaze.add(Self.wander("transform.translation.y", [0, 0.025 * side, 0], period: 0.9), forKey: "nod")
        case .thinking: Self.ponder(gaze: gaze, eye: eyes[0], side: side)
        case .reading: Self.scan(gaze: gaze, side: side)
        case .working: eyes.forEach { $0.add(Self.typing(), forKey: "typing") }
        case .stuck: gaze.add(Self.wander("opacity", [0, -0.3, 0], period: 3), forKey: "breath")
        case .stopping: eyes.forEach { $0.add(Self.closing(), forKey: "close") }
        case .resting, .listening, .asking, .off: break
        }
    }

    @MainActor
    private static func ponder(gaze: CALayer, eye: CALayer, side: CGFloat) {
        gaze.add(wander("transform.translation.x", [0, -0.03 * side, 0.01 * side, 0], period: 2.8), forKey: "ponder.x")
        gaze.add(wander("transform.translation.y", [0, -0.02 * side, 0.02 * side, 0], period: 2.8), forKey: "ponder.y")
        eye.add(wander("transform.scale.y", [0, 0.25, 0], period: 2.8), forKey: "ponder")
    }

    @MainActor
    private static func scan(gaze: CALayer, side: CGFloat) {
        let scan = CAKeyframeAnimation(keyPath: "transform.translation.x")
        scan.values = [0, 0.14, 0, 0.14, 0, 0.14, 0].map { $0 * side }
        scan.keyTimes = [0, 0.28, 0.33, 0.61, 0.66, 0.94, 1]
        scan.isAdditive = true
        gaze.add(scan.looping(period: 2.7), forKey: "scan")
        let lines = CAKeyframeAnimation(keyPath: "transform.translation.y")
        lines.values = [0, 0, 0.04, 0.04, 0.08, 0.08, 0].map { $0 * side }
        lines.keyTimes = [0, 0.3, 0.33, 0.63, 0.66, 0.96, 1]
        lines.isAdditive = true
        gaze.add(lines.looping(period: 2.7), forKey: "lines")
    }

    private static func typing() -> CAKeyframeAnimation {
        let typing = CAKeyframeAnimation(keyPath: "opacity")
        typing.values = [1, 0.3, 0.3, 1, 1].map { Float($0) }
        typing.keyTimes = [0, 0.08, 0.44, 0.52, 1]
        return typing.looping(period: 1)
    }

    private static func closing() -> CABasicAnimation {
        let close = CABasicAnimation(keyPath: "transform.scale.y")
        close.fromValue = 1
        close.toValue = 0.2
        close.duration = 0.5
        close.fillMode = .forwards
        close.isRemovedOnCompletion = false
        close.timingFunction = CAMediaTimingFunction(name: .easeIn)
        return close
    }

    private static func blink(period: TimeInterval) -> CAKeyframeAnimation {
        let blink = CAKeyframeAnimation(keyPath: "transform.scale.y")
        blink.values = [1, 1, 0.1, 1]
        blink.keyTimes = [0, 0.94, 0.97, 1]
        return blink.looping(period: period)
    }

    private static func wander(_ keyPath: String, _ values: [CGFloat], period: TimeInterval) -> CAKeyframeAnimation {
        let wander = CAKeyframeAnimation(keyPath: keyPath)
        wander.values = values
        wander.isAdditive = true
        wander.calculationMode = .cubic
        return wander.looping(period: period)
    }
}

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
