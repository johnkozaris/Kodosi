pragma Singleton

import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    readonly property bool isDark: Models.Appearance.dark
    readonly property bool reduceMotion: Models.Appearance.reduceMotion

    readonly property color ground: isDark ? "#110e0c" : "#f0e9dc"
    readonly property color surface: isDark ? "#181411" : "#f7f2e8"
    readonly property color raised: isDark ? "#251d19" : "#fefbf5"
    readonly property color lifted: isDark ? "#2e241f" : "#fffdf9"
    readonly property color well: isDark ? "#0c0a08" : "#e8e0d2"
    readonly property color hairline: isDark ? "#3b2f29" : "#d9ccba"
    readonly property color ink: isDark ? "#f1e9e3" : "#25160e"
    readonly property color inkMuted: isDark ? "#b3a094" : "#5f4838"
    readonly property color inkFaint: isDark ? "#8c7a6f" : "#8a7262"
    readonly property color accent: isDark ? "#db8a62" : "#b85a26"
    readonly property color accentHover: isDark ? "#e69b74" : "#c5672f"
    readonly property color accentStrong: isDark ? "#eaa47e" : "#9a4516"
    readonly property color accentSoft: isDark ? "#3e281c" : "#f4dec8"
    readonly property color accentInk: isDark ? "#160e0a" : "#fffaf3"
    readonly property color glowAmber: isDark ? "#f2bc55" : "#f2c14e"
    readonly property color glowOrange: isDark ? "#f08a4b" : "#e8793a"
    readonly property color glowRose: isDark ? "#e8707a" : "#dd5f6d"
    readonly property color ready: isDark ? "#7dbb99" : "#2f7d54"
    readonly property color readySoft: isDark ? "#1f3229" : "#dcebdd"
    readonly property color caution: isDark ? "#ddb866" : "#8a6a1c"
    readonly property color cautionSoft: isDark ? "#3a2f17" : "#f3e6bf"
    readonly property color danger: isDark ? "#e77a75" : "#b23a32"
    readonly property color dangerSoft: isDark ? "#3f211f" : "#f4d9d4"
    readonly property color dangerInk: isDark ? "#1a0c0b" : "#fffaf3"
    readonly property color shadow: isDark ? "#000000" : "#2e1f0f"
    readonly property real shadowStrength: isDark ? 1 : 0.36
    readonly property color highlight: isDark ? Qt.rgba(1, 0.945, 0.863, 0.11) : Qt.rgba(1, 1, 1, 0.95)
    readonly property color lowlight: isDark ? Qt.rgba(0, 0, 0, 0.34) : Qt.rgba(0.31, 0.19, 0.09, 0.12)

    readonly property color terminal: "#0d0b09"
    readonly property color terminalBand: "#1a1512"
    readonly property color terminalBandSelected: "#221a16"
    readonly property color terminalInk: "#f1e9e3"
    readonly property color terminalInkMuted: "#b3a094"
    readonly property color terminalInkFaint: "#8c7a6f"
    readonly property color terminalHairline: "#3b2f29"
    readonly property color terminalAccent: "#db8a62"

    readonly property int fontLarge: 28
    readonly property int fontTitle: 20
    readonly property int fontHeadline: 15
    readonly property int fontCallout: 14
    readonly property int fontBody: 13
    readonly property int fontFootnote: 12
    readonly property int fontCaption: 11
    readonly property int fontCaption2: 10

    readonly property real radiusXs: 6
    readonly property real radiusSm: 8
    readonly property real radiusMd: 10
    readonly property real radiusLg: 14
    readonly property real radiusXl: 18
    readonly property real radiusSheet: 22
    readonly property real radiusTile: 12

    readonly property real sidebarWidth: 252
    readonly property real railWidth: 78
    readonly property real frameInset: 8
    readonly property real headerHeight: 54
    readonly property real controlHeight: 32
    readonly property real fieldHeight: 34

    readonly property int motionHover: reduceMotion ? 0 : 120
    readonly property int motionFade: reduceMotion ? 0 : 200
    readonly property int motionSnappy: reduceMotion ? 0 : 240
    readonly property int motionSpring: reduceMotion ? 0 : 360
    readonly property int motionSoft: reduceMotion ? 0 : 480
    readonly property real overshoot: 1.15

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
}
