import SwiftUI

struct AppTheme {
    let colors: Colors
    let motion: Motion
    let isDark: Bool

    static let dark = AppTheme(colors: .dark, motion: Motion(reduced: false), isDark: true)
    static let light = AppTheme(colors: .light, motion: Motion(reduced: false), isDark: false)

    func reducingMotion(_ reduced: Bool) -> AppTheme {
        AppTheme(colors: colors, motion: Motion(reduced: reduced), isDark: isDark)
    }
}

extension AppTheme {
    struct Colors {
        let ground: Color
        let surface: Color
        let raised: Color
        let lifted: Color
        let well: Color
        let terminal: Color
        let hairline: Color

        let ink: Color
        let inkMuted: Color
        let inkFaint: Color

        let accent: Color
        let accentHover: Color
        let accentStrong: Color
        let accentSoft: Color
        let onAccent: Color

        let glowAmber: Color
        let glowOrange: Color
        let glowRose: Color

        let ready: Color
        let readySoft: Color
        let caution: Color
        let cautionSoft: Color
        let danger: Color
        let dangerSoft: Color
        let onDanger: Color

        let highlight: Color
        let lowlight: Color
        let shadow: Color

        static let dark = Colors(
            ground: hex(0x110E0C),
            surface: hex(0x181411),
            raised: hex(0x251D19),
            lifted: hex(0x2E241F),
            well: hex(0x0C0A08),
            terminal: hex(0x0D0B09),
            hairline: hex(0x3B2F29),

            ink: hex(0xF1E9E3),
            inkMuted: hex(0xB3A094),
            inkFaint: hex(0x8C7A6F),

            accent: hex(0xDB8A62),
            accentHover: hex(0xE69B74),
            accentStrong: hex(0xEAA47E),
            accentSoft: hex(0x3E281C),
            onAccent: hex(0x160E0A),

            glowAmber: hex(0xF2BC55),
            glowOrange: hex(0xF08A4B),
            glowRose: hex(0xE8707A),

            ready: hex(0x7DBB99),
            readySoft: hex(0x1F3229),
            caution: hex(0xDDB866),
            cautionSoft: hex(0x3A2F17),
            danger: hex(0xE77A75),
            dangerSoft: hex(0x3F211F),
            onDanger: hex(0x1A0C0B),

            highlight: Color(red: 1, green: 0.945, blue: 0.863).opacity(0.09),
            lowlight: Color.black.opacity(0.3),
            shadow: Color.black
        )

        static let light = Colors(
            ground: hex(0xF0E9DC),
            surface: hex(0xF7F2E8),
            raised: hex(0xFEFBF5),
            lifted: hex(0xFFFDF9),
            well: hex(0xE8E0D2),
            terminal: hex(0x0D0B09),
            hairline: hex(0xD9CCBA),

            ink: hex(0x25160E),
            inkMuted: hex(0x5F4838),
            inkFaint: hex(0x8A7262),

            accent: hex(0xB85A26),
            accentHover: hex(0xC5672F),
            accentStrong: hex(0x9A4516),
            accentSoft: hex(0xF4DEC8),
            onAccent: hex(0xFFFAF3),

            glowAmber: hex(0xF2C14E),
            glowOrange: hex(0xE8793A),
            glowRose: hex(0xDD5F6D),

            ready: hex(0x2F7D54),
            readySoft: hex(0xDCEBDD),
            caution: hex(0x8A6A1C),
            cautionSoft: hex(0xF3E6BF),
            danger: hex(0xB23A32),
            dangerSoft: hex(0xF4D9D4),
            onDanger: hex(0xFFFAF3),

            highlight: Color.white.opacity(0.95),
            lowlight: Color(red: 0.31, green: 0.19, blue: 0.09).opacity(0.06),
            shadow: Color(red: 0.18, green: 0.12, blue: 0.06)
        )
    }
}

extension AppTheme {
    struct Motion {
        let reduced: Bool

        var spring: Animation? {
            reduced ? nil : .interpolatingSpring(mass: 0.9, stiffness: 420, damping: 34)
        }

        var soft: Animation? {
            reduced ? nil : .interpolatingSpring(stiffness: 260, damping: 30)
        }

        var snappy: Animation? {
            reduced ? nil : .interpolatingSpring(stiffness: 620, damping: 40)
        }

        var fade: Animation? {
            reduced ? nil : .easeOut(duration: 0.2)
        }

        var hover: Animation? {
            reduced ? nil : .easeOut(duration: 0.12)
        }
    }
}

enum Radius {
    static let xs: CGFloat = 6
    static let sm: CGFloat = 8
    static let md: CGFloat = 10
    static let lg: CGFloat = 14
    static let xl: CGFloat = 18
    static let sheet: CGFloat = 22
}

enum Metrics {
    static let sidebarWidth: CGFloat = 252
    static let sidebarRailWidth: CGFloat = 78
    static let frameInset: CGFloat = 8
    static let headerHeight: CGFloat = 54
    static let titleBarHeight: CGFloat = 36
}

func hex(_ value: UInt32) -> Color {
    Color(
        red: Double((value >> 16) & 0xFF) / 255,
        green: Double((value >> 8) & 0xFF) / 255,
        blue: Double(value & 0xFF) / 255
    )
}

extension EnvironmentValues {
    @Entry var theme: AppTheme = .dark
    @Entry var chromeTheme: AppTheme?
}
