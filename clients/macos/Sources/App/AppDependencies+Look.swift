import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

extension AppDependencies {
    /// The look of this Mac's terminals for everyone who sees them: the app's appearance, unless
    /// the person keeps them dark.
    var terminalLook: TerminalLook {
        let appearance = themePreference.resolve(systemColorScheme: systemDark ? .dark : .light)
        return settings.keepsTerminalsDark || appearance.isDark ? .dark : .light
    }

    /// The terminals run on while no window shows, so the look follows the app and not a window.
    func watchAppearance() {
        appearanceWatch = NSApp.observe(\.effectiveAppearance, options: [.initial, .new]) { [weak self] _, change in
            let dark = change.newValue?.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            MainActor.assumeIsolated { self?.systemChanged(dark: dark) }
        }
        readThemePreference()
        _ = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification, object: storageBootstrap.defaults, queue: .main,
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.readThemePreference()
                self?.reportLook()
            }
        }
    }

    func reportLook() {
        let look = terminalLook
        guard look != reportedLook,
              CommandSink.didAcceptSynchronously(commandSink.send("app.look", ["look": .string(look.rawValue)]))
        else { return }
        reportedLook = look
    }

    private func systemChanged(dark: Bool) {
        guard systemDark != dark else { return }
        systemDark = dark
        reportLook()
    }

    private func readThemePreference() {
        let preference = ThemePreference(rawValue: storageBootstrap.defaults.string(forKey: ThemePreference.storageKey) ?? "") ?? .system
        if preference != themePreference {
            themePreference = preference
        }
    }
}
