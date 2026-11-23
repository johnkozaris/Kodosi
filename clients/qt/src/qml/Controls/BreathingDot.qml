import Kodosi 1.0
import QtQuick

Item {
    id: root

    property real size: 7
    property color color: KodosiTheme.accent

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    SignPhase {
        id: breath
        period: 1600
        running: !KodosiTheme.reduceMotion && KodosiTheme.shown && root.visible
    }
    Rectangle {
        readonly property real spread: 1 - (1 - breath.value) * (1 - breath.value)

        anchors.centerIn: parent
        width: root.size
        height: root.size
        radius: width / 2
        color: "transparent"
        border.width: 1.5
        border.color: root.color
        scale: 1 + 1.4 * spread
        opacity: breath.running ? 0.55 * (1 - spread) : 0
    }
    Rectangle {
        anchors.centerIn: parent
        width: root.size
        height: root.size
        radius: width / 2
        color: root.color
    }
}
