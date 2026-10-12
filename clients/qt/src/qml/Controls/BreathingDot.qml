import Kodosi 1.0
import QtQuick

Item {
    id: root

    property color color: KodosiTheme.accent
    property real size: 7

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Rectangle {
        id: halo
        anchors.centerIn: parent
        width: root.size
        height: root.size
        radius: width / 2
        color: root.color
        opacity: 0.35

        ParallelAnimation {
            running: !KodosiTheme.reduceMotion && root.visible
            loops: Animation.Infinite
            SequentialAnimation {
                NumberAnimation { target: halo; property: "scale"; from: 1; to: 2.4; duration: 1300; easing.type: Easing.OutCubic }
                PauseAnimation { duration: 500 }
            }
            SequentialAnimation {
                NumberAnimation { target: halo; property: "opacity"; from: 0.4; to: 0; duration: 1300; easing.type: Easing.OutCubic }
                PauseAnimation { duration: 500 }
            }
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
