import Kodosi 1.0
import QtQuick

Item {
    id: root

    property real radius: KodosiTheme.radiusMd
    property color fill: KodosiTheme.accent
    property bool quiet: false
    property bool pressed: false
    property bool lifted: true
    property color face: pressed ? KodosiTheme.mix(fill, "#000000", 0.08) : fill

    readonly property bool dark: KodosiTheme.isDark
    readonly property real hair: Screen.devicePixelRatio >= 2 ? 0.5 : 1
    readonly property real strength: lifted && !dark ? 1 : 0
    readonly property real line: 1 / Math.max(4, height)
    readonly property color light: Qt.rgba(1, 1, 1, dark ? (quiet ? 0.06 : 0.05) : (quiet ? 0.35 : 0.22))
    readonly property color shade: Qt.rgba(0, 0, 0, dark ? 0.2 : 0)

    Behavior on face { ColorAnimation { duration: KodosiTheme.motionHover } }

    Rectangle {
        x: -0.5
        y: 1.5
        width: root.width + 1
        height: root.height
        radius: root.radius + 0.5
        color: KodosiTheme.shadow
        opacity: Math.min(1, (root.quiet ? 0.05 : 0.12) * root.strength)
    }
    Rectangle {
        y: 1
        width: root.width
        height: root.height
        radius: root.radius
        color: KodosiTheme.shadow
        opacity: Math.min(1, (root.quiet ? 0.12 : 0.3) * root.strength)
    }
    Rectangle {
        x: root.quiet ? -root.hair : 0
        y: x
        width: root.width - x * 2
        height: root.height - x * 2
        radius: root.radius - x
        visible: !root.dark
        color: root.quiet
            ? KodosiTheme.alpha(KodosiTheme.shadow, 0.22)
            : Qt.tint(root.face, Qt.rgba(0, 0, 0, 0.18))
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: root.quiet || root.dark ? 0 : root.hair
        radius: root.radius - anchors.margins
        color: root.face
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        gradient: Gradient {
            GradientStop { position: 0; color: root.light }
            GradientStop { position: root.line; color: root.light }
            GradientStop { position: root.line + 0.001; color: "transparent" }
            GradientStop { position: 0.999 - root.line; color: "transparent" }
            GradientStop { position: 1 - root.line; color: root.shade }
            GradientStop { position: 1; color: root.shade }
        }
    }
}
