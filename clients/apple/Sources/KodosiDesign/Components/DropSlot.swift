import SwiftUI

/// The outline of a place that takes what a person drags, which shows while it is over the place.
public struct DropSlot: View {
    @Environment(\.theme) private var theme
    let radius: CGFloat
    let shown: Bool
    var label: String?

    public init(radius: CGFloat, shown: Bool, label: String? = nil) {
        self.radius = radius
        self.shown = shown
        self.label = label
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        shape.fill(theme.colors.accentSoft.opacity(shown ? 0.55 : 0))
            .overlay { shape.strokeBorder(theme.colors.accent.opacity(shown ? 1 : 0), style: StrokeStyle(lineWidth: label == nil ? 1.5 : 2, dash: [5, 4])) }
            .overlay {
                if shown, let label {
                    VStack(spacing: 10) {
                        Image(systemName: "arrow.down").font(.system(size: 18, weight: .bold)).foregroundStyle(theme.colors.onAccent)
                            .frame(width: 48, height: 48).background(theme.colors.accent, in: Circle())
                        Text(label).appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                    }
                    .transition(theme.motion.reduced ? .opacity : .scale(scale: 0.6).combined(with: .opacity))
                }
            }
            .scaleEffect(shown || theme.motion.reduced ? 1 : 0.97)
            .animation(theme.motion.spring, value: shown)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}
