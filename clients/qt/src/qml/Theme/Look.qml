import QtQuick

QtObject {
    required property bool dark
    readonly property string name: dark ? "dark" : "light"

    readonly property color ground: dark ? "#110e0c" : "#f0e9dc"
    readonly property color surface: dark ? "#181411" : "#f7f2e8"
    readonly property color raised: dark ? "#251d19" : "#fefbf5"
    readonly property color lifted: dark ? "#2e241f" : "#fffdf9"
    readonly property color well: dark ? "#0c0a08" : "#e8e0d2"
    readonly property color sheet: Qt.rgba(raised.r + (surface.r - raised.r) * 0.45, raised.g + (surface.g - raised.g) * 0.45,
        raised.b + (surface.b - raised.b) * 0.45, 1)
    readonly property color hairline: dark ? "#3b2f29" : "#d9ccba"
    readonly property color ink: dark ? "#f1e9e3" : "#25160e"
    readonly property color inkMuted: dark ? "#b3a094" : "#5f4838"
    readonly property color inkFaint: dark ? "#8c7a6f" : "#8a7262"
    readonly property color accent: dark ? "#db8a62" : "#b85a26"
    readonly property color accentHover: dark ? "#e69b74" : "#c5672f"
    readonly property color accentStrong: dark ? "#eaa47e" : "#9a4516"
    readonly property color accentSoft: dark ? "#3e281c" : "#f4dec8"
    readonly property color accentInk: dark ? "#160e0a" : "#fffaf3"
    readonly property color ready: dark ? "#7dbb99" : "#2f7d54"
    readonly property color readySoft: dark ? "#1f3229" : "#dcebdd"
    readonly property color caution: dark ? "#ddb866" : "#8a6a1c"
    readonly property color cautionSoft: dark ? "#3a2f17" : "#f3e6bf"
    readonly property color danger: dark ? "#e77a75" : "#b23a32"
    readonly property color dangerSoft: dark ? "#3f211f" : "#f4d9d4"
    readonly property color dangerInk: dark ? "#1a0c0b" : "#fffaf3"
    readonly property color glowAmber: dark ? "#f2bc55" : "#f2c14e"
    readonly property color glowOrange: dark ? "#f08a4b" : "#e8793a"
    readonly property color shadow: dark ? "#000000" : "#2e1f0f"
    readonly property real shadowStrength: dark ? 1 : 0.36
    readonly property color highlight: dark ? Qt.rgba(1, 0.945, 0.863, 0.11) : Qt.rgba(1, 1, 1, 0.95)
    readonly property color lowlight: dark ? Qt.rgba(0, 0, 0, 0.34) : Qt.rgba(0.31, 0.19, 0.09, 0.12)

    readonly property color terminal: dark ? "#0d0b09" : "#faf6ee"
    readonly property color band: dark ? "#1a1512" : "#f1eadf"
    readonly property color bandSelected: dark ? "#221a16" : "#fefbf5"
    readonly property color bandHover: dark ? "#2a221e" : "#e5ded4"

    function buttonFace(lit: bool): color {
        return Qt.tint(lifted, Qt.rgba(ink.r, ink.g, ink.b, lit ? (dark ? 0.05 : 0.03) : 0))
    }
}
