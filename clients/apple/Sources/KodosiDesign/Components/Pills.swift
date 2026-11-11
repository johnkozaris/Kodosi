import SwiftUI

public struct SegmentOption<Value: Hashable>: Identifiable {
    public let value: Value
    public let title: String
    public var symbol: String?
    public var count: Int?

    public init(value: Value, title: String, symbol: String? = nil, count: Int? = nil) {
        self.value = value
        self.title = title
        self.symbol = symbol
        self.count = count
    }

    public var id: Value {
        value
    }
}

private enum Segment {
    #if os(iOS)
        static let height: CGFloat = 34
        static let compactHeight: CGFloat = 30
        static let padding: CGFloat = 14
        static let compactPadding: CGFloat = 12
    #else
        static let height: CGFloat = 28
        static let compactHeight: CGFloat = 24
        static let padding: CGFloat = 13
        static let compactPadding: CGFloat = 10
    #endif
}

public struct SegmentedPill<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    #if os(iOS)
        @Environment(\.dynamicTypeSize) private var typeSize
    #endif
    @Namespace private var thumb
    @Binding public var selection: Value
    public let options: [SegmentOption<Value>]
    public var compact = false
    public var identifier = "segment"

    public init(selection: Binding<Value>, options: [SegmentOption<Value>], compact: Bool = false, identifier: String = "segment") {
        _selection = selection
        self.options = options
        self.compact = compact
        self.identifier = identifier
    }

    /// At the accessibility text sizes of iPhone and iPad, the choices stand under each other.
    private var stacked: Bool {
        #if os(iOS)
            typeSize.isAccessibilitySize
        #else
            false
        #endif
    }

    public var body: some View {
        if stacked {
            VStack(spacing: 2) { segments }
                .padding(3)
                .well(Radius.xl)
        } else {
            HStack(spacing: 2) { segments }
                .padding(3)
                .wellCapsule()
        }
    }

    private var segments: some View {
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
                    #if os(iOS)
                        .lineLimit(stacked ? nil : 1)
                        .minimumScaleFactor(stacked ? 1 : 0.7)
                    #endif
                    if let count = option.count, count > 0 {
                        CountBadge(count: count, quiet: true)
                    }
                }
                .appTextStyle(compact ? .footnote : .subhead)
                .fontWeight(.medium)
                .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
                .padding(.horizontal, compact ? Segment.compactPadding : Segment.padding)
                .controlHeight(compact ? Segment.compactHeight : Segment.height)
                .frame(maxWidth: stacked ? .infinity : nil)
                .background {
                    if selected {
                        thumbFace.matchedGeometryEffect(id: "thumb", in: thumb)
                    }
                }
                .touchTarget(Capsule())
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(selected ? .isSelected : [])
            .accessibilityIdentifier("\(identifier).\(String(describing: option.value))")
        }
    }

    @ViewBuilder
    private var thumbFace: some View {
        if stacked {
            RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.xl - 3, style: .continuous), fill: theme.colors.raised, elevation: .resting)
        } else {
            RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
        }
    }
}

public struct DockItem: Identifiable, Sendable {
    public let id: Int
    public let title: String
    public let symbol: String
    public var count = 0
    public var progress: Double?
    public var fresh = false
    public var key: String?

    public init(
        id: Int, title: String, symbol: String, count: Int = 0, progress: Double? = nil, fresh: Bool = false, key: String? = nil,
    ) {
        self.id = id
        self.title = title
        self.symbol = symbol
        self.count = count
        self.progress = progress
        self.fresh = fresh
        self.key = key
    }
}

public struct SectionDock: View {
    #if os(iOS)
        private static let height: CGFloat = 34
        private static let padding: CGFloat = 14
    #else
        private static let height: CGFloat = 28
        private static let padding: CGFloat = 11
    #endif

    @Environment(\.theme) private var theme
    @Namespace private var thumb
    public let items: [DockItem]
    public let selection: Int
    public var identifier = "dock"
    public let select: (Int) -> Void

    public init(items: [DockItem], selection: Int, identifier: String = "dock", select: @escaping (Int) -> Void) {
        self.items = items
        self.selection = selection
        self.identifier = identifier
        self.select = select
    }

    public var body: some View {
        HStack(spacing: 2) {
            ForEach(items) { item in
                let selected = item.id == selection
                Button {
                    withAnimation(theme.motion.snappy) { select(item.id) }
                } label: {
                    HStack(spacing: 6) {
                        glyph(item, selected: selected).hop(when: item.fresh)
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
                    .padding(.horizontal, Self.padding)
                    .controlHeight(Self.height)
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
                    .touchTarget(Capsule())
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)
                .help(item.title)
                .accessibilityLabel(Text(item.title))
                .accessibilityValue(item.count > 0 ? Text(item.count, format: .number) : Text(verbatim: ""))
                .accessibilityAddTraits(selected ? .isSelected : [])
                .accessibilityIdentifier("\(identifier).\(item.key ?? String(item.id))")
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

public struct ProgressRing: View {
    @Environment(\.theme) private var theme
    public let value: Double
    public var size: CGFloat = 16
    public var lineWidth: CGFloat = 2.5

    public init(value: Double, size: CGFloat = 16, lineWidth: CGFloat = 2.5) {
        self.value = value
        self.size = size
        self.lineWidth = lineWidth
    }

    public var body: some View {
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

public struct CountBadge: View {
    @Environment(\.theme) private var theme
    public let count: Int
    public var quiet = false

    public init(count: Int, quiet: Bool = false) {
        self.count = count
        self.quiet = quiet
    }

    public var body: some View {
        Text(verbatim: count > 99 ? "99+" : "\(count)")
            .font(.system(size: 10, weight: .semibold, design: .rounded)).monospacedDigit()
            .contentTransition(.numericText())
            .foregroundStyle(quiet ? theme.colors.inkMuted : theme.colors.onAccent)
            .padding(.horizontal, 5)
            .frame(minWidth: 17, minHeight: 17)
            .background(quiet ? theme.colors.ink.opacity(0.1) : theme.colors.accent, in: Capsule())
            .animation(theme.motion.snappy, value: count)
    }
}

public struct NewDot: View {
    @Environment(\.theme) private var theme

    public init() {}

    public var body: some View {
        Circle().fill(theme.colors.accent).frame(width: 8, height: 8)
    }
}

public struct Tag: View {
    public enum Tone: Sendable { case neutral, accent, ready, caution, danger }

    @Environment(\.theme) private var theme
    public let text: String
    public var symbol: String?
    public var tone: Tone = .neutral

    public init(text: String, symbol: String? = nil, tone: Tone = .neutral) {
        self.text = text
        self.symbol = symbol
        self.tone = tone
    }

    public var body: some View {
        HStack(spacing: 4) {
            if let symbol {
                Image(systemName: symbol).imageScale(.small).fontWeight(.bold)
            }
            Text(text).lineLimit(1)
        }
        .appTextStyle(.caption)
        .foregroundStyle(foreground)
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .frame(minHeight: 20)
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
