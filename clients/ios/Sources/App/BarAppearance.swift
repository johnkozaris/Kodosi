import KodosiDesign
import SwiftUI
import UIKit

/// The system bars in the app's type and colors: titles in the text styles of the app, and the
/// counts on the tabs in copper.
enum BarAppearance {
    static func apply() {
        let ink = color(\.ink)
        let navigation = UINavigationBarAppearance()
        navigation.largeTitleTextAttributes = [
            .font: font(.largeTitle, size: 34),
            .kern: -0.6,
            .foregroundColor: ink,
        ]
        navigation.titleTextAttributes = [
            .font: font(.headline, size: 17),
            .kern: -0.2,
            .foregroundColor: ink,
        ]
        let bar = UINavigationBar.appearance()
        bar.standardAppearance = navigation
        bar.scrollEdgeAppearance = navigation
        bar.compactAppearance = navigation
        bar.compactScrollEdgeAppearance = navigation

        let tabs = UITabBarAppearance()
        for layout in [tabs.stackedLayoutAppearance, tabs.inlineLayoutAppearance, tabs.compactInlineLayoutAppearance] {
            for state in [layout.normal, layout.selected] {
                state.badgeBackgroundColor = color(\.accent)
                state.badgeTextAttributes = [.foregroundColor: color(\.onAccent)]
            }
        }
        UITabBar.appearance().standardAppearance = tabs
        UITabBar.appearance().scrollEdgeAppearance = tabs
    }

    private static func font(_ style: UIFont.TextStyle, size: CGFloat) -> UIFont {
        UIFontMetrics(forTextStyle: style).scaledFont(for: .systemFont(ofSize: size, weight: .semibold))
    }

    private static func color(_ role: KeyPath<AppTheme.Colors, Color>) -> UIColor {
        let light = UIColor(AppTheme.light.colors[keyPath: role])
        let dark = UIColor(AppTheme.dark.colors[keyPath: role])
        return UIColor { $0.userInterfaceStyle == .dark ? dark : light }
    }
}
