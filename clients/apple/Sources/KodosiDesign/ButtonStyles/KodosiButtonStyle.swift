import SwiftUI

public struct KodosiButtonStyle: ButtonStyle {
    public enum Kind: Sendable { case primary, secondary, ghost, tinted, danger }
    public enum Size: Sendable {
        case small, regular, large

        public var height: CGFloat {
            #if os(iOS)
                switch self {
                case .small: 34
                case .regular: 44
                case .large: 52
                }
            #else
                switch self {
                case .small: 26
                case .regular: 32
                case .large: 40
                }
            #endif
        }

        public var padding: CGFloat {
            #if os(iOS)
                switch self {
                case .small: 14
                case .regular: 18
                case .large: 22
                }
            #else
                switch self {
                case .small: 11
                case .regular: 14
                case .large: 18
                }
            #endif
        }

        public var radius: CGFloat {
            #if os(iOS)
                switch self {
                case .small: 10
                case .regular: 13
                case .large: 16
                }
            #else
                switch self {
                case .small: Radius.sm
                case .regular: Radius.md
                case .large: 12
                }
            #endif
        }

        public var style: AppTextStyle {
            switch self {
            case .small: .footnote
            case .regular: .subhead
            case .large: .headline
            }
        }
    }

    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false
    public var kind: Kind = .secondary
    public var size: Size = .regular

    public init(kind: Kind = .secondary, size: Size = .regular) {
        self.kind = kind
        self.size = size
    }

    public func makeBody(configuration: Configuration) -> some View {
        let shape = RoundedRectangle(cornerRadius: size.radius, style: .continuous)
        let pressed = configuration.isPressed
        configuration.label
            .appTextStyle(size.style)
            .fontWeight(.semibold)
            .lineLimit(1)
            .foregroundStyle(foreground(pressed: pressed))
            .padding(.horizontal, size.padding)
            .frame(minHeight: size.height)
            .background { background(shape, pressed: pressed).environment(\.bezelPressed, pressed) }
            .contentShape(shape)
            .scaleEffect(pressed ? 0.97 : 1)
            .opacity(isEnabled ? 1 : 0.45)
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
            .animation(theme.motion.snappy, value: pressed)
    }

    private var lit: Bool {
        hovered && isEnabled
    }

    private func foreground(pressed: Bool) -> Color {
        switch kind {
        case .primary: theme.colors.onAccent
        case .secondary: theme.colors.ink
        case .ghost: lit || pressed ? theme.colors.ink : theme.colors.inkMuted
        case .tinted: theme.colors.accentStrong
        case .danger: theme.colors.onDanger
        }
    }

    @ViewBuilder
    private func background(_ shape: RoundedRectangle, pressed: Bool) -> some View {
        let colors = theme.colors
        switch kind {
        case .primary:
            BezelBackground(shape: shape, fill: lit ? colors.accentHover : colors.accent)
        case .secondary:
            BezelBackground(shape: shape, fill: theme.buttonFace(lit: lit), bezel: .quiet)
        case .ghost:
            shape.fill(colors.ink.opacity(pressed ? 0.13 : lit ? 0.08 : 0))
        case .tinted:
            BezelBackground(shape: shape, fill: colors.accentSoft.mix(with: colors.accent, by: lit ? 0.14 : 0), bezel: .quiet)
        case .danger:
            BezelBackground(shape: shape, fill: colors.danger.mix(with: .white, by: lit ? 0.12 : 0))
        }
    }
}

public extension ButtonStyle where Self == KodosiButtonStyle {
    static func kodosi(_ kind: KodosiButtonStyle.Kind, size: KodosiButtonStyle.Size = .regular) -> KodosiButtonStyle {
        KodosiButtonStyle(kind: kind, size: size)
    }
}

public extension AppTheme {
    func buttonFace(lit: Bool = false) -> Color {
        colors.lifted.mix(with: colors.ink, by: (isDark ? 0.07 : 0) + (lit ? 0.03 : 0))
    }
}

public struct IconButton: View {
    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false
    public let title: LocalizedStringKey
    public let symbol: String
    public let identifier: String
    public var size: CGFloat = 28
    public var destructive = false
    public var active = false
    public let action: () -> Void

    public init(
        title: LocalizedStringKey,
        symbol: String,
        identifier: String,
        size: CGFloat = 28,
        destructive: Bool = false,
        active: Bool = false,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.symbol = symbol
        self.identifier = identifier
        self.size = size
        self.destructive = destructive
        self.active = active
        self.action = action
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: min(Radius.md, size * 0.32), style: .continuous)
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size * 0.43, weight: .semibold))
                .contentTransition(.symbolEffect(.replace))
                .foregroundStyle(foreground)
                .frame(width: size, height: size)
                .background {
                    if let face {
                        BezelBackground(shape: shape, fill: face, bezel: .quiet).transition(.opacity)
                    }
                }
                .contentShape(shape)
        }
        .buttonStyle(PressScaleStyle())
        .opacity(isEnabled ? 1 : 0.35)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .tip(title).accessibilityLabel(Text(title)).accessibilityIdentifier(identifier)
    }

    private var lit: Bool {
        hovered && isEnabled
    }

    private var foreground: Color {
        if lit, destructive {
            return theme.colors.danger
        }
        if active {
            return theme.colors.accentStrong
        }
        return lit ? theme.colors.ink : theme.colors.inkMuted
    }

    private var face: Color? {
        if lit, destructive {
            return theme.colors.dangerSoft
        }
        if active {
            return theme.colors.accentSoft
        }
        return lit ? theme.buttonFace(lit: true) : nil
    }
}

public struct PressScaleStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    public var scale: CGFloat = 0.95

    public init(scale: CGFloat = 0.95) {
        self.scale = scale
    }

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .environment(\.bezelPressed, configuration.isPressed)
            .scaleEffect(configuration.isPressed ? scale : 1)
            .animation(theme.motion.snappy, value: configuration.isPressed)
    }
}

private struct HoverRow: ViewModifier {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let radius: CGFloat
    let selected: Bool

    func body(content: Content) -> some View {
        content
            .background {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(theme.colors.ink.opacity(hovered && !selected ? 0.06 : 0))
            }
            .contentShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
    }
}

public extension View {
    func hoverRow(radius: CGFloat = Radius.md, selected: Bool = false) -> some View {
        modifier(HoverRow(radius: radius, selected: selected))
    }
}
