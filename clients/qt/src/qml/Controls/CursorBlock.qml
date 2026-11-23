import Kodosi 1.0
import QtQuick

Item {
    id: root

    property bool blinks: false
    property color color: KodosiTheme.accent

    implicitWidth: 8
    implicitHeight: 16
    Accessible.ignored: true

    SignPhase {
        id: blink
        period: 1250
        running: root.blinks && !KodosiTheme.reduceMotion && KodosiTheme.shown && root.visible
    }
    Rectangle {
        readonly property real at: blink.value * blink.period

        anchors.fill: parent
        radius: Math.min(width, height) * 0.2
        color: root.color
        opacity: !blink.running || at < 580 ? 1
            : at < 680 ? 1 - 0.8 * (at - 580) / 100
            : at < 1150 ? 0.2
            : 0.2 + 0.8 * (at - 1150) / 100
    }
}
