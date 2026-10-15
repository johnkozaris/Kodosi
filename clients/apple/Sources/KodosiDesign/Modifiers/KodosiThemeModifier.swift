import SwiftUI

private struct KodosiThemeModifier: ViewModifier {
    @Environment(\.colorScheme) private var systemColorScheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage(ThemePreference.storageKey) private var preference: String = ThemePreference.system.rawValue
    @AppStorage(MotionPreference.storageKey) private var reducesMotion = false

    private var resolved: AppTheme {
        let pref = ThemePreference(rawValue: preference) ?? .system
        return pref.resolve(systemColorScheme: systemColorScheme).reducingMotion(reduceMotion || reducesMotion)
    }

    private var colorSchemeOverride: ColorScheme? {
        let pref = ThemePreference(rawValue: preference) ?? .system
        return pref.preferredColorScheme
    }

    func body(content: Content) -> some View {
        let theme = resolved
        content
            .transaction { transaction in
                if theme.motion.reduced {
                    transaction.animation = nil
                    transaction.disablesAnimations = true
                }
            }
            .environment(\.theme, theme)
            .tint(theme.colors.accent)
            .background(theme.colors.ground.ignoresSafeArea())
            .preferredColorScheme(colorSchemeOverride)
    }
}

public extension View {
    func kodosiTheme() -> some View {
        modifier(KodosiThemeModifier())
    }
}
