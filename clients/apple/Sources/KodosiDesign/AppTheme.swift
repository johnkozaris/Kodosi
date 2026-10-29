import SwiftUI

public struct AppTheme: Sendable {
    public let colors: Colors
    public let motion: Motion
    public let isDark: Bool

    public init(colors: Colors, motion: Motion, isDark: Bool) {
        self.colors = colors
        self.motion = motion
        self.isDark = isDark
    }

    public static let dark = AppTheme(colors: .dark, motion: Motion(reduced: false), isDark: true)
    public static let light = AppTheme(colors: .light, motion: Motion(reduced: false), isDark: false)

    public func reducingMotion(_ reduced: Bool) -> AppTheme {
        AppTheme(colors: colors, motion: Motion(reduced: reduced), isDark: isDark)
    }
}

public extension AppTheme {
    struct Colors: Sendable {
        public let ground: Color
        public let surface: Color
        public let raised: Color
        public let lifted: Color
        public let well: Color
        public let terminal: Color
        public let hairline: Color

        public let ink: Color
        public let inkMuted: Color
        public let inkFaint: Color

        public let accent: Color
        public let accentHover: Color
        public let accentStrong: Color
        public let accentSoft: Color
        public let onAccent: Color

        public let glowAmber: Color
        public let glowOrange: Color

        public let ready: Color
        public let readySoft: Color
        public let caution: Color
        public let cautionSoft: Color
        public let danger: Color
        public let dangerSoft: Color
        public let onDanger: Color

        public let highlight: Color
        public let lowlight: Color
        public let shadow: Color

        public static let dark = Colors(
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

            ready: hex(0x7DBB99),
            readySoft: hex(0x1F3229),
            caution: hex(0xDDB866),
            cautionSoft: hex(0x3A2F17),
            danger: hex(0xE77A75),
            dangerSoft: hex(0x3F211F),
            onDanger: hex(0x1A0C0B),

            highlight: Color(red: 1, green: 0.945, blue: 0.863).opacity(0.09),
            lowlight: Color.black.opacity(0.3),
            shadow: Color.black,
        )

        public static let light = Colors(
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

            ready: hex(0x2F7D54),
            readySoft: hex(0xDCEBDD),
            caution: hex(0x8A6A1C),
            cautionSoft: hex(0xF3E6BF),
            danger: hex(0xB23A32),
            dangerSoft: hex(0xF4D9D4),
            onDanger: hex(0xFFFAF3),

            highlight: Color.white.opacity(0.95),
            lowlight: Color(red: 0.31, green: 0.19, blue: 0.09).opacity(0.06),
            shadow: Color(red: 0.18, green: 0.12, blue: 0.06),
        )
    }
}

public extension AppTheme.Colors {
    /// The ground of what shows beside the work: a room's conversation and a canvas.
    var sheet: Color {
        raised.mix(with: surface, by: 0.45)
    }
}

public extension AppTheme {
    struct Motion: Sendable {
        public let reduced: Bool

        public init(reduced: Bool) {
            self.reduced = reduced
        }

        public static let springShape = Spring(mass: 0.9, stiffness: 420, damping: 34)
        public static let softShape = Spring(stiffness: 260, damping: 30)
        public static let snappyShape = Spring(stiffness: 620, damping: 40)

        public var spring: Animation? {
            reduced ? nil : .interpolatingSpring(Self.springShape)
        }

        public var soft: Animation? {
            reduced ? nil : .interpolatingSpring(Self.softShape)
        }

        public var snappy: Animation? {
            reduced ? nil : .interpolatingSpring(Self.snappyShape)
        }

        public static let outCurve: [Float] = [0.23, 1, 0.32, 1]
        public static let outUnit = UnitCurve.bezier(startControlPoint: UnitPoint(x: 0.23, y: 1), endControlPoint: UnitPoint(x: 0.32, y: 1))

        public var fade: Animation? {
            reduced ? nil : .timingCurve(0.23, 1, 0.32, 1, duration: 0.2)
        }

        public var hover: Animation? {
            reduced ? nil : .timingCurve(0.25, 0.1, 0.25, 1, duration: 0.12)
        }
    }
}

public enum Radius {
    public static let xs: CGFloat = 6
    public static let sm: CGFloat = 8
    public static let md: CGFloat = 10
    public static let lg: CGFloat = 14
    public static let xl: CGFloat = 18
    public static let sheet: CGFloat = 22
}

public func hex(_ value: UInt32) -> Color {
    Color(
        red: Double((value >> 16) & 0xFF) / 255,
        green: Double((value >> 8) & 0xFF) / 255,
        blue: Double(value & 0xFF) / 255,
    )
}

public extension EnvironmentValues {
    @Entry var theme: AppTheme = .dark
    @Entry var chromeTheme: AppTheme?
    @Entry var bezelPressed = false
}
