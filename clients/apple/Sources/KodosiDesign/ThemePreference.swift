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
