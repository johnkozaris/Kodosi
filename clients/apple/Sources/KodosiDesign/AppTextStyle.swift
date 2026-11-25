import SwiftUI

public enum AppTextStyle: Sendable {
    case large
    case title
    case headline
    case subhead
    case callout
    case body
    case footnote
    case caption
    case caption2
    case mono
    case monoCaption

    fileprivate func font(_ theme: AppTheme) -> Font {
        #if os(iOS)
            textStyleFont
        #else
            .system(size: theme.scaled(points), weight: weight, design: design)
        #endif
    }

    #if os(iOS)
        private var textStyleFont: Font {
            switch self {
            case .large: .system(.largeTitle, weight: .semibold)
            case .title: .system(.title2, weight: .semibold)
            case .headline: .system(.headline, weight: .semibold)
            case .subhead: .system(.subheadline, weight: .semibold)
            case .callout: .system(.callout)
            case .body: .system(.body)
            case .footnote: .system(.footnote)
            case .caption: .system(.caption, weight: .medium)
            case .caption2: .system(.caption2, weight: .medium)
            case .mono: .system(.subheadline, design: .monospaced)
            case .monoCaption: .system(.caption, design: .monospaced)
            }
        }
    #else
        private var points: CGFloat {
            switch self {
            case .large: 28
            case .title: 20
            case .headline: 15
            case .callout: 14
            case .subhead, .body: 13
            case .mono: 12.5
            case .footnote: 12
            case .caption, .monoCaption: 11
            case .caption2: 10
            }
        }

        private var weight: Font.Weight {
            switch self {
            case .large, .title, .headline, .subhead: .semibold
            case .caption, .caption2: .medium
            case .callout, .body, .footnote, .mono, .monoCaption: .regular
            }
        }

        private var design: Font.Design {
            switch self {
            case .mono, .monoCaption: .monospaced
            case .large, .title, .headline, .subhead, .callout, .body, .footnote, .caption, .caption2: .default
            }
        }
    #endif

    fileprivate var tracking: CGFloat {
        switch self {
        case .large: -0.6
        case .title: -0.4
        case .headline: -0.2
        case .subhead, .callout, .body: -0.08
        case .footnote, .caption, .caption2, .mono, .monoCaption: 0
        }
    }
}

private struct AppTextStyleFont: ViewModifier {
    @Environment(\.theme) private var theme
    let style: AppTextStyle

    func body(content: Content) -> some View {
        content.font(style.font(theme)).tracking(style.tracking * theme.textScale)
    }
}

private struct ScaledFont: ViewModifier {
    @Environment(\.theme) private var theme
    let size: CGFloat
    let style: Font.TextStyle?
    let weight: Font.Weight
    let design: Font.Design

    func body(content: Content) -> some View {
        #if os(iOS)
            if let style {
                content.font(.system(style, design: design, weight: weight))
            } else {
                content.font(.system(size: size, weight: weight, design: design))
            }
        #else
            content.font(.system(size: theme.scaled(size), weight: weight, design: design))
        #endif
    }
}

/// Exactly this height on a Mac, at the person's text size, and at least this on iPhone and iPad,
/// where the text grows with Dynamic Type.
private struct ControlHeight: ViewModifier {
    @Environment(\.theme) private var theme
    let height: CGFloat

    func body(content: Content) -> some View {
        #if os(iOS)
            content.frame(minHeight: height)
        #else
            content.frame(height: theme.scaled(height))
        #endif
    }
}

public extension View {
    func appTextStyle(_ style: AppTextStyle) -> some View {
        modifier(AppTextStyleFont(style: style))
    }

    /// `size` points at the person's text size on a Mac, and the same points on iPhone and iPad.
    func font(scaled size: CGFloat, weight: Font.Weight = .regular, design: Font.Design = .default) -> some View {
        modifier(ScaledFont(size: size, style: nil, weight: weight, design: design))
    }

    /// `size` points at the person's text size on a Mac, and the text style `style` on iPhone and
    /// iPad, where it grows with Dynamic Type.
    func font(mac size: CGFloat, ios style: Font.TextStyle, weight: Font.Weight = .regular, design: Font.Design = .default) -> some View {
        modifier(ScaledFont(size: size, style: style, weight: weight, design: design))
    }

    func controlHeight(_ height: CGFloat) -> some View {
        modifier(ControlHeight(height: height))
    }
}
