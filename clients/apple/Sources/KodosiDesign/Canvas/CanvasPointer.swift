import KodosiBridge
import SwiftUI

/// The agent's pointer over the shared browser: it glides to the element that the agent acts
/// on, and a ring opens once when it clicks. It leaves a moment after the agent stops, when the
/// page changes, or when a person on this device uses the browser.
struct AgentPointerLayer: View {
    @Environment(\.theme) private var theme
    let browser: CanvasBrowser
    let feed: BrowserFeed
    @State private var shown: BrowserDoing?
    @State private var clicks = 0

    var body: some View {
        GeometryReader { geometry in
            if let shown, let point = shown.point, let at = feed.viewPoint(point, in: geometry.size) {
                let kind = AgentKind(agentName: shown.agent)
                ZStack(alignment: .topLeading) {
                    ClickRing(color: kind.tint, trigger: clicks)
                        .offset(x: at.x - ClickRing.size / 2, y: at.y - ClickRing.size / 2)
                    AgentArrow(color: kind.tint, pressed: browser.doing?.kind == .click)
                        .offset(x: at.x, y: at.y)
                    AgentMark(kind: kind, size: 18, activity: browser.doing == nil ? .awake : .working)
                        .offset(x: at.x + 13, y: at.y + 17)
                }
                .animation(theme.motion.spring, value: at)
                .transition(.opacity)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .onChange(of: browser.doing) { _, doing in
            guard let doing, doing.point != nil else { return }
            withAnimation(shown == nil ? nil : theme.motion.spring) { shown = doing }
            if doing.kind == .click {
                clicks += 1
            }
        }
        .onChange(of: feed.touches) {
            withAnimation(theme.motion.fade) { shown = nil }
        }
        .onChange(of: browser.activeTab?.url) {
            withAnimation(theme.motion.fade) { shown = nil }
        }
        .task(id: browser.doing == nil) {
            guard browser.doing == nil, shown != nil else { return }
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled else { return }
            withAnimation(theme.motion.fade) { shown = nil }
        }
    }
}

/// A pointer in the agent's color whose tip is at the top left of its frame.
private struct AgentArrow: View {
    let color: Color
    let pressed: Bool

    var body: some View {
        ArrowShape()
            .fill(color)
            .overlay { ArrowShape().stroke(.white, style: StrokeStyle(lineWidth: 1.4, lineJoin: .round)) }
            .frame(width: 15, height: 21)
            .shadow(color: .black.opacity(0.35), radius: 1.5, x: 0.6, y: 1.2)
            .scaleEffect(pressed ? 0.86 : 1, anchor: .topLeading)
    }
}

private struct ArrowShape: Shape {
    func path(in rect: CGRect) -> Path {
        let width = rect.width / 15
        let height = rect.height / 21
        var path = Path()
        for (index, corner) in [(0.0, 0.0), (0, 17), (4.3, 13.2), (7.4, 20), (10.4, 18.7), (7.4, 12.1), (13, 12.1)].enumerated() {
            let point = CGPoint(x: rect.minX + corner.0 * width, y: rect.minY + corner.1 * height)
            if index == 0 {
                path.move(to: point)
            } else {
                path.addLine(to: point)
            }
        }
        path.closeSubpath()
        return path
    }
}

/// A ring that opens once at a click.
private struct ClickRing: View {
    static let size: CGFloat = 34

    @Environment(\.theme) private var theme
    let color: Color
    let trigger: Int

    var body: some View {
        Circle().stroke(color, lineWidth: 2)
            .frame(width: Self.size, height: Self.size)
            .keyframeAnimator(initialValue: Ring(), trigger: trigger) { view, ring in
                view.scaleEffect(ring.scale).opacity(ring.opacity)
            } keyframes: { _ in
                KeyframeTrack(\.scale) {
                    LinearKeyframe(0.3, duration: 0.24)
                    CubicKeyframe(1.25, duration: 0.42)
                }
                KeyframeTrack(\.opacity) {
                    LinearKeyframe(0, duration: 0.24)
                    LinearKeyframe(0.9, duration: 0.04)
                    CubicKeyframe(0, duration: 0.38)
                }
            }
            .opacity(theme.motion.reduced ? 0 : 1)
    }

    struct Ring {
        var scale = 0.3
        var opacity = 0.0
    }
}
