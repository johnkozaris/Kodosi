import SwiftUI

enum AppTextStyle {
    case headingDisplay
    case headingSection
    case headingItem
    case body
    case caption
    case metadata
    case eyebrow
    case panelLabel
    case mono
    case monoCaption
    case button

    fileprivate var font: Font {
        switch self {
        case .headingDisplay:
            .system(.title3, weight: .semibold)
        case .headingSection:
            .system(.headline, weight: .semibold)
        case .headingItem:
            .system(.headline, weight: .semibold)
        case .body:
            .body
        case .caption:
            .callout
        case .metadata:
            .caption
        case .eyebrow:
            .system(.subheadline, weight: .medium)
        case .panelLabel:
            .system(.callout, weight: .bold)
        case .mono:
            .system(.body, design: .monospaced)
        case .monoCaption:
            .system(.callout, design: .monospaced)
        case .button:
            .system(.callout, weight: .semibold)
        }
    }
}

private struct AppTextStyleModifier: ViewModifier {
    let style: AppTextStyle

    func body(content: Content) -> some View {
        content.font(style.font)
    }
}

extension View {
    func appTextStyle(_ style: AppTextStyle) -> some View {
        modifier(AppTextStyleModifier(style: style))
    }
}
