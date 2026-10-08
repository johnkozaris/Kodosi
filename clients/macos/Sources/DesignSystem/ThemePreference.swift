import SwiftUI

enum ThemePreference: String {
    case system
    case dark
    case light

    static let storageKey = "themePreference"

    func resolve(systemColorScheme: ColorScheme) -> AppTheme {
        switch self {
        case .system: systemColorScheme == .dark ? .dark : .light
        case .dark: .dark
        case .light: .light
        }
    }

    var preferredColorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .dark: .dark
        case .light: .light
        }
    }
}
