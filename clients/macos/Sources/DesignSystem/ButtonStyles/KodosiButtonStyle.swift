import SwiftUI

struct KodosiButtonStyle: ButtonStyle {
    enum Kind { case primary, secondary, ghost, tinted, danger }
    enum Size {
        case small, regular, large

        var height: CGFloat {
            switch self {
            case .small: 26
            case .regular: 32
            case .large: 40
            }
        }

        var padding: CGFloat {
            switch self {
            case .small: 11
            case .regular: 14
            case .large: 18
            }
        }

        var style: AppTextStyle {
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
    var kind: Kind = .secondary
    var size: Size = .regular

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .appTextStyle(size.style)
            .fontWeight(.semibold)
            .lineLimit(1)
            .foregroundStyle(foreground)
            .padding(.horizontal, size.padding)
            .frame(minHeight: size.height)
            .background { background(pressed: configuration.isPressed) }
            .contentShape(Capsule())
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .opacity(isEnabled ? 1 : 0.45)
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
            .animation(theme.motion.snappy, value: configuration.isPressed)
    }

    private var foreground: Color {
        switch kind {
        case .primary: theme.colors.onAccent
        case .secondary: theme.colors.ink
        case .ghost: hovered ? theme.colors.ink : theme.colors.inkMuted
        case .tinted: theme.colors.accentStrong
        case .danger: theme.colors.onDanger
        }
    }

    @ViewBuilder
    private func background(pressed: Bool) -> some View {
        switch kind {
        case .primary:
            RaisedBackground(shape: Capsule(), fill: hovered ? theme.colors.accentHover : theme.colors.accent,
                             elevation: pressed ? .flat : .resting)
        case .secondary:
            RaisedBackground(shape: Capsule(), fill: hovered ? theme.colors.lifted : theme.colors.raised,
                             elevation: pressed ? .flat : .resting)
        case .ghost:
            Capsule().fill(theme.colors.ink.opacity(hovered ? 0.08 : 0))
        case .tinted:
            Capsule().fill(hovered ? theme.colors.accent.opacity(0.28) : theme.colors.accentSoft)
        case .danger:
            RaisedBackground(shape: Capsule(), fill: theme.colors.danger.opacity(hovered ? 0.9 : 1),
                             elevation: pressed ? .flat : .resting)
        }
    }
}

extension ButtonStyle where Self == KodosiButtonStyle {
    static func kodosi(_ kind: KodosiButtonStyle.Kind, size: KodosiButtonStyle.Size = .regular) -> KodosiButtonStyle {
        KodosiButtonStyle(kind: kind, size: size)
    }
}

struct IconButton: View {
    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false
    let title: LocalizedStringKey
    let symbol: String
    let identifier: String
    var size: CGFloat = 28
    var destructive = false
    var active = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size * 0.43, weight: .semibold))
                .contentTransition(.symbolEffect(.replace))
                .foregroundStyle(foreground)
                .frame(width: size, height: size)
                .background { Circle().fill(fill) }
                .contentShape(Circle())
        }
        .buttonStyle(PressScaleStyle())
        .opacity(isEnabled ? 1 : 0.35)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .help(title).accessibilityLabel(Text(title)).accessibilityIdentifier(identifier)
    }

    private var foreground: Color {
        if hovered, destructive {
            return theme.colors.danger
        }
        if active {
            return theme.colors.accentStrong
        }
        return hovered ? theme.colors.ink : theme.colors.inkMuted
    }

    private var fill: Color {
        if hovered, destructive {
            return theme.colors.danger.opacity(0.14)
        }
        if active {
            return theme.colors.accentSoft
        }
        return theme.colors.ink.opacity(hovered ? 0.09 : 0)
    }
}

struct PressScaleStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    var scale: CGFloat = 0.94

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? scale : 1)
            .animation(theme.motion.snappy, value: configuration.isPressed)
    }
}

struct HoverRow: ViewModifier {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    var radius: CGFloat = Radius.md
    var selected = false

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

extension View {
    func hoverRow(radius: CGFloat = Radius.md, selected: Bool = false) -> some View {
        modifier(HoverRow(radius: radius, selected: selected))
    }
}
