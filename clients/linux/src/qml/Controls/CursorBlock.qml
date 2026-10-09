import Kodosi 1.0
import QtQuick

Item {
    id: root

    property bool blinks: false
    property color color: KodosiTheme.accent

    implicitWidth: 8
    implicitHeight: 16
    Accessible.ignored: true

    Rectangle {
        anchors.fill: parent
        anchors.margins: -3
        radius: 4
        color: root.color
        opacity: 0.22 * block.opacity
    }
    Rectangle {
        id: block
        anchors.fill: parent
        radius: Math.min(width, height) * 0.2
        color: root.color

        SequentialAnimation on opacity {
            running: root.blinks && !KodosiTheme.reduceMotion && root.visible
            loops: Animation.Infinite
            alwaysRunToEnd: true
            PauseAnimation { duration: 520 }
            NumberAnimation { to: 0.2; duration: 90 }
            PauseAnimation { duration: 420 }
            NumberAnimation { to: 1; duration: 90 }
        }
    }
}
