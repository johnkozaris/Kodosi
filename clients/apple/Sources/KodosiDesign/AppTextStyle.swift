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

    fileprivate var font: Font {
        #if os(iOS)
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
        #else
            switch self {
            case .large: .system(size: 28, weight: .semibold)
            case .title: .system(size: 20, weight: .semibold)
            case .headline: .system(size: 15, weight: .semibold)
            case .subhead: .system(size: 13, weight: .semibold)
            case .callout: .system(size: 14)
            case .body: .system(size: 13)
            case .footnote: .system(size: 12)
            case .caption: .system(size: 11, weight: .medium)
            case .caption2: .system(size: 10, weight: .medium)
            case .mono: .system(size: 12.5, design: .monospaced)
            case .monoCaption: .system(size: 11, design: .monospaced)
            }
        #endif
    }

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

public extension View {
    func appTextStyle(_ style: AppTextStyle) -> some View {
        font(style.font).tracking(style.tracking)
    }
}

extension View {
    /// The height of a control around text: exactly this on a Mac, and at least this on iPhone
    /// and iPad, where the text grows with Dynamic Type.
    func controlHeight(_ height: CGFloat) -> some View {
        #if os(iOS)
            frame(minHeight: height)
        #else
            frame(height: height)
        #endif
    }

    /// A font of `size` points on a Mac, and of the text style `style` on iPhone and iPad, where
    /// it grows with Dynamic Type.
    func font(mac size: CGFloat, ios style: Font.TextStyle, weight: Font.Weight = .regular, design: Font.Design = .default) -> some View {
        #if os(iOS)
            font(.system(style, design: design, weight: weight))
        #else
            font(.system(size: size, weight: weight, design: design))
        #endif
    }
}
