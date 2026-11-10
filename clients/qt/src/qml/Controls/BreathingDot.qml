import Kodosi 1.0
import QtQuick

Item {
    id: root

    property real size: 7
    property color color: KodosiTheme.accent

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Rectangle {
        id: ring
        anchors.centerIn: parent
        width: root.size
        height: root.size
        radius: width / 2
        color: "transparent"
        border.width: 1.5
        border.color: root.color
        opacity: 0

        ParallelAnimation {
            running: !KodosiTheme.reduceMotion && KodosiTheme.shown && root.visible
            loops: Animation.Infinite
            NumberAnimation { target: ring; property: "scale"; from: 1; to: 2.4; duration: 1600; easing.type: Easing.OutQuad }
            NumberAnimation { target: ring; property: "opacity"; from: 0.55; to: 0; duration: 1600; easing.type: Easing.OutQuad }
            onStopped: ring.opacity = 0
        }
    }
    Rectangle {
        anchors.centerIn: parent
        width: root.size
        height: root.size
        radius: width / 2
        color: root.color
    }
}
