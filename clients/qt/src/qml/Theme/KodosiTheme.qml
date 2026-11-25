pragma Singleton

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    readonly property bool isDark: Models.Appearance.dark
    readonly property bool reduceMotion: Models.Appearance.reduceMotion
    readonly property real textScale: Models.Appearance.textScale

    /// `size` at the person's text size, for words and the symbols beside them.
    function scaled(size) {
        return Math.round(size * textScale)
    }
    property bool shown: true

    readonly property Look darkLook: Look { dark: true }
    readonly property Look lightLook: Look { dark: false }
    readonly property Look look: isDark ? darkLook : lightLook
    readonly property Look terminalLook: lookNamed(Models.Appearance.terminalLook)

    readonly property color ground: look.ground
    readonly property color surface: look.surface
    readonly property color raised: look.raised
    readonly property color lifted: look.lifted
    readonly property color well: look.well
    readonly property color sheet: look.sheet
    readonly property color hairline: look.hairline
    readonly property color ink: look.ink
    readonly property color inkMuted: look.inkMuted
    readonly property color inkFaint: look.inkFaint
    readonly property color accent: look.accent
    readonly property color accentHover: look.accentHover
    readonly property color accentStrong: look.accentStrong
    readonly property color accentSoft: look.accentSoft
    readonly property color accentInk: look.accentInk
    readonly property color ready: look.ready
    readonly property color readySoft: look.readySoft
    readonly property color caution: look.caution
    readonly property color cautionSoft: look.cautionSoft
    readonly property color danger: look.danger
    readonly property color dangerSoft: look.dangerSoft
    readonly property color glowAmber: look.glowAmber
    readonly property color glowOrange: look.glowOrange
    readonly property color shadow: look.shadow
    readonly property real shadowStrength: look.shadowStrength
    readonly property color highlight: look.highlight
    readonly property color lowlight: look.lowlight

    readonly property int fontLarge: Math.round(28 * textScale)
    readonly property int fontTitle: Math.round(20 * textScale)
    readonly property int fontHeadline: Math.round(15 * textScale)
    readonly property int fontCallout: Math.round(14 * textScale)
    readonly property int fontBody: Math.round(13 * textScale)
    readonly property int fontFootnote: Math.round(12 * textScale)
    readonly property int fontCaption: Math.round(11 * textScale)
    readonly property int fontCaption2: Math.round(10 * textScale)

    readonly property real radiusXs: 6
    readonly property real radiusSm: 8
    readonly property real radiusMd: 10
    readonly property real radiusLg: 14
    readonly property real radiusXl: 18
    readonly property real radiusSheet: 22
    readonly property real radiusTile: 12

    readonly property real frameInset: 8
    readonly property real titleBarHeight: 36
    readonly property real headerHeight: 54
    readonly property real controlHeight: Math.round(32 * textScale)
    readonly property real fieldHeight: Math.round(34 * textScale)

    readonly property int motionHover: reduceMotion ? 0 : 120
    readonly property int motionFade: reduceMotion ? 0 : 200
    readonly property int motionSnappy: reduceMotion ? 0 : 240
    readonly property int motionSpring: reduceMotion ? 0 : 360
    readonly property int motionSoft: reduceMotion ? 0 : 480
    readonly property real overshoot: 0.6
    readonly property var outCurve: [0.23, 1, 0.32, 1, 1, 1]

    function mix(base: color, tint: color, amount: real): color {
        return Qt.rgba(
            base.r + (tint.r - base.r) * amount,
            base.g + (tint.g - base.g) * amount,
            base.b + (tint.b - base.b) * amount,
            base.a + (tint.a - base.a) * amount)
    }

    function alpha(base: color, amount: real): color {
        return Qt.rgba(base.r, base.g, base.b, amount)
    }

    function buttonFace(lit: bool): color {
        return look.buttonFace(lit)
    }

    function lookNamed(name: string): Look {
        return name === "light" ? lightLook : darkLook
    }

    // The outline of a rounded rectangle with Apple's continuous corners, as SVG path data.
    // Each corner bends from 1.53 radii along one edge to 1.53 radii along the next.
    function squircle(x: real, y: real, width: real, height: real, radius: real): string {
        const r = Math.max(0, Math.min(radius, Math.min(width, height) / 3.05732966))
        const curve = [[1.52866483, 0], [1.08849323, 0], [0.86840689, 0.02189748], [0.66993427, 0.065496],
            [0.63149399, 0.074911], [0.37282392, 0.16905899], [0.16905899, 0.37282392], [0.074911, 0.63149399],
            [0.065496, 0.66993427], [0.02189748, 0.86840689], [0, 1.08849323], [0, 1.52866483]]
        const right = x + width
        const bottom = y + height
        const corners = [
            (u, v) => [right - u * r, y + v * r],
            (u, v) => [right - v * r, bottom - u * r],
            (u, v) => [x + u * r, bottom - v * r],
            (u, v) => [x + v * r, y + u * r]
        ]
        const point = (corner, index) => corner(curve[index][0], curve[index][1]).map(value => value.toFixed(3)).join(" ")
        let path = "M " + point(corners[3], 11)
        for (const corner of corners) {
            path += " L " + point(corner, 0) + " C " + point(corner, 1) + " " + point(corner, 2) + " " + point(corner, 3)
                + " L " + point(corner, 4) + " C " + point(corner, 5) + " " + point(corner, 6) + " " + point(corner, 7)
                + " L " + point(corner, 8) + " C " + point(corner, 9) + " " + point(corner, 10) + " " + point(corner, 11)
        }
        return path + " Z"
    }
}
