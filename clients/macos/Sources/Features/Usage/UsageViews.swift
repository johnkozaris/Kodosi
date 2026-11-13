import KodosiBridge
import KodosiDesign
import SwiftUI

extension UsageLevel {
    func color(_ colors: AppTheme.Colors) -> Color {
        switch self {
        case .normal: colors.inkMuted
        case .near: colors.caution
        case .reached: colors.danger
        }
    }
}

struct UsageRing: View {
    @Environment(\.theme) private var theme
    let limit: UsageLimit?
    var lineWidth: CGFloat = 2
    var loading = false
    @State private var drawn = 0.0

    private var fraction: Double {
        Double(limit?.used ?? 0) / 100
    }

    var body: some View {
        ZStack {
            Circle().inset(by: lineWidth / 2).stroke(
                theme.colors.ink.opacity(limit == nil ? 0.24 : 0.12),
                style: StrokeStyle(lineWidth: lineWidth, dash: limit == nil && !loading ? [lineWidth * 1.1, lineWidth * 1.5] : []),
            )
            if let limit, limit.used > 0 {
                Circle().inset(by: lineWidth / 2)
                    .trim(from: 0, to: drawn)
                    .stroke(limit.level.color(theme.colors), style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
            if limit == nil, loading {
                LoadingArc(lineWidth: lineWidth).transition(.opacity)
            }
        }
        .onAppear {
            withAnimation(theme.motion.soft?.delay(0.06)) { drawn = fraction }
        }
        .onChange(of: fraction) { _, now in
            withAnimation(theme.motion.soft) { drawn = now }
        }
        .animation(theme.motion.fade, value: limit?.level)
        .animation(theme.motion.fade, value: loading)
        .accessibilityHidden(true)
    }
}

private struct LoadingArc: View {
    @Environment(\.theme) private var theme
    let lineWidth: CGFloat
    @State private var turning = false

    var body: some View {
        Circle().inset(by: lineWidth / 2)
            .trim(from: 0, to: 0.24)
            .stroke(theme.colors.inkMuted, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
            .rotationEffect(.degrees(turning ? 270 : -90))
            .animation(turning ? .linear(duration: 1.1).repeatForever(autoreverses: false) : nil, value: turning)
            .onAppear { turning = !theme.motion.reduced }
            .onChange(of: theme.motion.reduced) { _, reduced in turning = !reduced }
    }
}

struct UsageDial: View {
    @Environment(\.theme) private var theme
    let limit: UsageLimit?
    var size: CGFloat = 26
    var loading = false

    var body: some View {
        ZStack {
            UsageRing(limit: limit, lineWidth: max(1.75, size * 0.085), loading: loading)
            if let limit {
                Text(verbatim: "\(limit.used)")
                    .font(.system(size: size * 0.37, weight: .semibold, design: .rounded).monospacedDigit())
                    .foregroundStyle(limit.level == .normal ? theme.colors.ink : limit.level.color(theme.colors))
                    .minimumScaleFactor(0.7)
                    .lineLimit(1)
                    .padding(size * 0.15)
                    .contentTransition(.numericText(value: Double(limit.used)))
                    .animation(theme.motion.snappy, value: limit.used)
            }
        }
        .frame(width: size, height: size)
        .squash(trigger: limit?.level, when: limit.map { $0.level != .normal } ?? false)
        .accessibilityElement()
        .accessibilityLabel(Text(verbatim: limit?.spoken ?? (loading ? String(localized: "Loading") : String(localized: "Not read yet"))))
    }
}

struct UsagePercent: View {
    @Environment(\.theme) private var theme
    let limit: UsageLimit

    var body: some View {
        Text(verbatim: "\(limit.used)%")
            .appTextStyle(.monoCaption)
            .foregroundStyle(limit.level == .normal ? theme.colors.inkFaint : limit.level.color(theme.colors))
            .contentTransition(.numericText(value: Double(limit.used)))
            .animation(theme.motion.snappy, value: limit.used)
    }
}
