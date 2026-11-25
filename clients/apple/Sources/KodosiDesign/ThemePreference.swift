import SwiftUI

public enum ThemePreference: String, Sendable {
    case system
    case dark
    case light

    public static let storageKey = "themePreference"

    public func resolve(systemColorScheme: ColorScheme) -> AppTheme {
        switch self {
        case .system: systemColorScheme == .dark ? .dark : .light
        case .dark: .dark
        case .light: .light
        }
    }

    public var preferredColorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .dark: .dark
        case .light: .light
        }
    }
}

public enum MotionPreference {
    public static let storageKey = "reducesMotion"
}

#if os(macOS)
    public enum TextSize: String, CaseIterable, Sendable {
        case small, normal, large, larger

        public static let storageKey = "textSize"

        public var scale: CGFloat {
            switch self {
            case .small: 0.92
            case .normal: 1
            case .large: 1.08
            case .larger: 1.16
            }
        }
    }
#endif

public enum SummaryFirst {
    public static let storageKey = "summaryFirst"
}
