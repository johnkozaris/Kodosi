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

/// The size of the words on a Mac. iPhone and iPad follow the text size of the system.
public enum TextSize: String, CaseIterable, Sendable {
    case small, normal, large, larger

    public static let storageKey = "textSize"

    /// About one point a step at the size of the conversation.
    public var scale: CGFloat {
        switch self {
        case .small: 0.92
        case .normal: 1
        case .large: 1.08
        case .larger: 1.16
        }
    }
}

public enum SummaryFirst {
    public static let storageKey = "summaryFirst"
}
