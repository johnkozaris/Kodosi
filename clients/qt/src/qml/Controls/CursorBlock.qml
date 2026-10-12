import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property bool blinks: false
    property color color: KodosiTheme.accent
    property real dim: 0
    readonly property bool pulsing: blinks && !KodosiTheme.reduceMotion && visible

    implicitWidth: 8
    implicitHeight: 16
    Accessible.ignored: true

    onPulsingChanged: {
        if (!pulsing)
            settle.restart()
    }

    Models.SoftShadow {
        anchors.fill: parent
        radius: block.radius
        blur: Math.max(1, root.width * 0.6)
        color: root.color
        opacity: 0.45 * (1 - root.dim)
    }
    Rectangle {
        id: block
        anchors.fill: parent
        radius: Math.max(1, root.width * 0.14)
        color: root.color
        opacity: 1 - 0.75 * root.dim
    }
    SequentialAnimation on dim {
        running: root.pulsing
        loops: Animation.Infinite

        NumberAnimation { to: 1; duration: 620; easing.type: Easing.InOutSine }
        NumberAnimation { to: 0; duration: 620; easing.type: Easing.InOutSine }
    }
    NumberAnimation { id: settle; target: root; property: "dim"; to: 0; duration: KodosiTheme.motionFade; easing.type: Easing.OutCubic }
}
