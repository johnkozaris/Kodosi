import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiTouch: Equatable {
    var count = 0
    var item: String?
    var tool: DosiTool?

    var shows: Bool {
        tool.map { [.type, .press, .interrupt].contains($0.action) } ?? false
    }
}

private struct DosiTouchCue: ViewModifier {
    @Environment(\.theme) private var theme
    let touch: DosiTouch
    let radius: CGFloat
    @State private var shown = false
    @State private var cards = 0

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        content
            .overlay {
                if !theme.motion.reduced {
                    shape.strokeBorder(theme.colors.glowAmber, lineWidth: 1.5)
                        .keyframeAnimator(initialValue: 0.0, trigger: touch.count) { view, value in
                            view.opacity(value)
                        } keyframes: { _ in
                            CubicKeyframe(1, duration: 0.12)
                            LinearKeyframe(1, duration: 0.3)
                            CubicKeyframe(0, duration: 0.9)
                        }
                        .allowsHitTesting(false)
                }
            }
            .overlay(alignment: .topTrailing) {
                if shown, touch.shows, let tool = touch.tool {
                    DosiTouchCard(tool: tool)
                        .padding(.top, 44).padding(.trailing, 12)
                        .transition(.opacity.combined(with: .offset(y: -8)).combined(with: .scale(scale: 0.96, anchor: .topTrailing)))
                        .allowsHitTesting(false)
                }
            }
            .onChange(of: touch.count) { _, _ in
                withAnimation(touch.shows ? theme.motion.spring : theme.motion.soft) { shown = touch.shows }
                cards += 1
            }
            .task(id: cards) {
                guard cards > 0 else { return }
                try? await Task.sleep(for: .seconds(2.6))
                guard !Task.isCancelled else { return }
                withAnimation(theme.motion.soft) { shown = false }
            }
    }
}

private struct DosiTouchCard: View {
    @Environment(\.theme) private var theme
    let tool: DosiTool

    var body: some View {
        HStack(spacing: 8) {
            DosiFace(mood: tool.status == .running ? .working : .resting, size: 20, still: true)
            Text(DosiWords.said(tool)).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(theme.colors.ink)
                .lineLimit(1).truncationMode(.middle).frame(maxWidth: 220, alignment: .leading).fixedSize()
                .contentTransition(.interpolate)
        }
        .padding(.leading, 7).padding(.trailing, 12).frame(height: 34)
        .animation(theme.motion.snappy, value: tool)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .floating)
        .accessibilityElement(children: .combine)
    }
}

extension View {
    func dosiTouch(_ touch: DosiTouch, radius: CGFloat) -> some View {
        modifier(DosiTouchCue(touch: touch, radius: radius))
    }
}
