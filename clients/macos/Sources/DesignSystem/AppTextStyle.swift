import SwiftUI

enum AppTextStyle {
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

extension View {
    func appTextStyle(_ style: AppTextStyle) -> some View {
        font(style.font).tracking(style.tracking)
    }
}
