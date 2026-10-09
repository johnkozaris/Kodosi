import SwiftUI

struct KodosiThemeModifier: ViewModifier {
    @Environment(\.colorScheme) private var systemColorScheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage(ThemePreference.storageKey) private var preference: String = ThemePreference.system.rawValue

    private var resolved: AppTheme {
        let pref = ThemePreference(rawValue: preference) ?? .system
        return pref.resolve(systemColorScheme: systemColorScheme).reducingMotion(reduceMotion)
    }

    private var colorSchemeOverride: ColorScheme? {
        let pref = ThemePreference(rawValue: preference) ?? .system
        return pref.preferredColorScheme
    }

    func body(content: Content) -> some View {
        content
            .environment(\.theme, resolved)
            .tint(resolved.colors.accent)
            .background(resolved.colors.ground.ignoresSafeArea())
            .preferredColorScheme(colorSchemeOverride)
    }
}

extension View {
    func kodosiTheme() -> some View {
        modifier(KodosiThemeModifier())
    }
}
