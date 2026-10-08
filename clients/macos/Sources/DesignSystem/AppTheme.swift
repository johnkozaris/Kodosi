import SwiftUI

struct AppTheme {
    let colors: Colors
    let radius: Radius
    let motion: Motion
    let isDark: Bool

    static let dark = AppTheme(
        colors: .dark,
        radius: .shared,
        motion: .shared,
        isDark: true
    )

    static let light = AppTheme(
        colors: .light,
        radius: .shared,
        motion: .shared,
        isDark: false
    )
}

extension AppTheme {
    struct Colors {
        let background: Color
        let foreground: Color
        let card: Color

        let primary: Color
        let primaryForeground: Color

        let secondary: Color
        let secondaryForeground: Color

        let muted: Color
        let mutedForeground: Color

        let accent: Color
        let accentForeground: Color

        let tertiary: Color
        let tertiaryForeground: Color

        let destructive: Color
        let destructiveForeground: Color

        let border: Color

        let surfacePanel: Color
        let surfaceStage: Color
        let surfaceTerminal: Color
        let seam: Color
        let shadowColor: Color

        let statusWaiting: Color

        static let light = Colors(
            background: hsl(38, 55, 95),
            foreground: hsl(22, 45, 10),
            card: hsl(36, 50, 90),

            primary: hsl(20, 75, 39),
            primaryForeground: hsl(40, 60, 97),

            secondary: hsl(32, 48, 84),
            secondaryForeground: hsl(22, 35, 16),

            muted: hsl(30, 38, 79),
            mutedForeground: hsl(22, 28, 28),

            accent: hsl(14, 70, 48),
            accentForeground: hsl(40, 60, 97),

            tertiary: hsl(148, 32, 36),
            tertiaryForeground: hsl(40, 60, 97),

            destructive: hsl(0, 60, 44),
            destructiveForeground: hsl(40, 60, 97),

            border: hsl(32, 38, 74),

            surfacePanel: hsl(35, 48, 88),
            surfaceStage: hsl(38, 52, 93),
            surfaceTerminal: hsl(28, 16, 89),
            seam: hsl(30, 32, 72),
            shadowColor: hsl(20, 30, 2),

            statusWaiting: hsl(38, 68, 42)
        )

        static let dark = Colors(
            background: hsl(20, 15, 6),
            foreground: hsl(33, 22, 90),
            card: hsl(20, 12, 14),

            primary: hsl(22, 65, 60),
            primaryForeground: hsl(20, 15, 6),

            secondary: hsl(20, 12, 17),
            secondaryForeground: hsl(33, 20, 82),

            muted: hsl(20, 12, 20),
            mutedForeground: hsl(28, 14, 56),

            accent: hsl(18, 60, 58),
            accentForeground: hsl(20, 15, 6),

            tertiary: hsl(150, 25, 48),
            tertiaryForeground: hsl(20, 15, 6),

            destructive: hsl(0, 60, 60),
            destructiveForeground: hsl(33, 22, 92),

            border: hsl(20, 10, 20),

            surfacePanel: hsl(20, 13, 11),
            surfaceStage: hsl(20, 14, 6),
            surfaceTerminal: hsl(18, 15, 3),
            seam: hsl(20, 10, 18),
            shadowColor: hsl(20, 20, 4),

            statusWaiting: hsl(38, 50, 56)
        )
    }
}

extension AppTheme {
    struct Radius {
        let sm: CGFloat = 3
        static let shared = Radius()
    }
}

extension AppTheme {
    struct Motion {
        let fast: Double = 0.15
        let selection: Animation = .spring(response: 0.3, dampingFraction: 0.88)
        let reveal: Animation = .smooth(duration: 0.22)
        static let shared = Motion()
    }
}

private func hsl(_ h: Double, _ s: Double, _ l: Double) -> Color {
    let sat = s / 100
    let light = l / 100
    let hue = h / 360

    let q = light < 0.5 ? light * (1 + sat) : light + sat - (light * sat)
    let p = 2 * light - q

    func convert(_ t: Double) -> Double {
        var v = t
        if v < 0 {
            v += 1
        }
        if v > 1 {
            v -= 1
        }
        if v < 1 / 6 {
            return p + (q - p) * 6 * v
        }
        if v < 0.5 {
            return q
        }
        if v < 2 / 3 {
            return p + (q - p) * (2 / 3 - v) * 6
        }
        return p
    }

    return Color(
        red: convert(hue + 1 / 3),
        green: convert(hue),
        blue: convert(hue - 1 / 3)
    )
}

extension EnvironmentValues {
    @Entry var theme: AppTheme = .dark
}
