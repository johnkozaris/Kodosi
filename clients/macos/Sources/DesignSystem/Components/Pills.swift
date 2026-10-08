import SwiftUI

struct SegmentOption<Value: Hashable>: Identifiable {
    let value: Value
    let title: String
    var symbol: String?
    var count: Int?

    var id: Value {
        value
    }
}

struct SegmentedPill<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    @Namespace private var thumb
    @Binding var selection: Value
    let options: [SegmentOption<Value>]
    var compact = false
    var identifier = "segment"

    var body: some View {
        HStack(spacing: 2) {
            ForEach(options) { option in
                let selected = option.value == selection
                Button {
                    withAnimation(theme.motion.snappy) { selection = option.value }
                } label: {
                    HStack(spacing: 6) {
                        if let symbol = option.symbol {
                            Image(systemName: symbol).font(.system(size: compact ? 11 : 12, weight: .semibold))
                        }
                        Text(option.title)
                        if let count = option.count, count > 0 {
                            CountBadge(count: count, quiet: true)
                        }
                    }
                    .appTextStyle(compact ? .footnote : .subhead)
                    .fontWeight(.medium)
                    .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
                    .padding(.horizontal, compact ? 10 : 13)
                    .frame(height: compact ? 24 : 28)
                    .background {
                        if selected {
                            RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                                .matchedGeometryEffect(id: "thumb", in: thumb)
                        }
                    }
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
                .accessibilityIdentifier("\(identifier).\(String(describing: option.value))")
            }
        }
        .padding(3)
        .wellCapsule()
    }
}

struct DockItem: Identifiable {
    let id: Int
    let title: String
    let symbol: String
    var count = 0
    var progress: Double?
    var fresh = false
}

struct SectionDock: View {
    @Environment(\.theme) private var theme
    @Namespace private var thumb
    let items: [DockItem]
    let selection: Int
    var identifier = "dock"
    let select: (Int) -> Void

    var body: some View {
        HStack(spacing: 2) {
            ForEach(items) { item in
                let selected = item.id == selection
                Button {
                    withAnimation(theme.motion.snappy) { select(item.id) }
                } label: {
                    HStack(spacing: 6) {
                        glyph(item, selected: selected)
                        if selected {
                            Text(item.title).fixedSize()
                                .transition(.opacity.combined(with: .offset(x: -6)))
                        } else if item.count > 0 {
                            Text(item.count, format: .number).monospacedDigit().contentTransition(.numericText())
                                .foregroundStyle(theme.colors.inkFaint)
                        }
                    }
                    .appTextStyle(.footnote)
                    .fontWeight(.medium)
                    .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
                    .padding(.horizontal, 11)
                    .frame(height: 28)
                    .background {
                        if selected {
                            RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                                .matchedGeometryEffect(id: "thumb", in: thumb)
                        }
                    }
                    .overlay(alignment: .topTrailing) {
                        if item.fresh, !selected {
                            BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -3, y: 3)
                                .transition(AnyTransition.pop)
                        }
                    }
                    .contentShape(Capsule())
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)
                .help(item.title)
                .accessibilityLabel(Text(item.title))
                .accessibilityValue(item.count > 0 ? Text(item.count, format: .number) : Text(verbatim: ""))
                .accessibilityAddTraits(selected ? .isSelected : [])
                .accessibilityIdentifier("\(identifier).\(item.id)")
            }
        }
        .padding(3)
        .wellCapsule()
        .animation(theme.motion.snappy, value: items.map(\.count))
    }

    @ViewBuilder
    private func glyph(_ item: DockItem, selected: Bool) -> some View {
        if let progress = item.progress {
            ProgressRing(value: progress, size: 14, lineWidth: 2.4)
        } else {
            Image(systemName: item.symbol).font(.system(size: 12, weight: .semibold))
                .foregroundStyle(selected ? theme.colors.accentStrong : theme.colors.inkMuted)
        }
    }
}

struct ProgressRing: View {
    @Environment(\.theme) private var theme
    let value: Double
    var size: CGFloat = 16
    var lineWidth: CGFloat = 2.5

    var body: some View {
        let done = value >= 1
        ZStack {
            Circle().stroke(theme.colors.ink.opacity(0.16), lineWidth: lineWidth)
            Circle().trim(from: 0, to: max(0, min(1, value)))
                .stroke(done ? theme.colors.ready : theme.colors.accent, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
        .animation(theme.motion.soft, value: value)
        .accessibilityHidden(true)
    }
}

struct CountBadge: View {
    @Environment(\.theme) private var theme
    let count: Int
    var quiet = false

    var body: some View {
        Text(count > 99 ? "99+" : "\(count)")
            .font(.system(size: 10, weight: .semibold, design: .rounded)).monospacedDigit()
            .contentTransition(.numericText())
            .foregroundStyle(quiet ? theme.colors.inkMuted : theme.colors.onAccent)
            .padding(.horizontal, 5)
            .frame(minWidth: 17, minHeight: 17)
            .background(quiet ? theme.colors.ink.opacity(0.1) : theme.colors.accent, in: Capsule())
            .animation(theme.motion.snappy, value: count)
    }
}

struct Tag: View {
    enum Tone { case neutral, accent, ready, caution, danger }

    @Environment(\.theme) private var theme
    let text: String
    var symbol: String?
    var tone: Tone = .neutral

    var body: some View {
        HStack(spacing: 4) {
            if let symbol {
                Image(systemName: symbol).font(.system(size: 9, weight: .bold))
            }
            Text(text).lineLimit(1)
        }
        .appTextStyle(.caption)
        .foregroundStyle(foreground)
        .padding(.horizontal, 8)
        .frame(height: 20)
        .background(background, in: Capsule())
    }

    private var foreground: Color {
        switch tone {
        case .neutral: theme.colors.inkMuted
        case .accent: theme.colors.accentStrong
        case .ready: theme.colors.ready
        case .caution: theme.colors.caution
        case .danger: theme.colors.danger
        }
    }

    private var background: Color {
        switch tone {
        case .neutral: theme.colors.ink.opacity(0.08)
        case .accent: theme.colors.accentSoft
        case .ready: theme.colors.readySoft
        case .caution: theme.colors.cautionSoft
        case .danger: theme.colors.dangerSoft
        }
    }
}
