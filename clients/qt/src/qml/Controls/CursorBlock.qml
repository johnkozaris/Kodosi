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
        id: block
        anchors.fill: parent
        radius: Math.min(width, height) * 0.2
        color: root.color
    }
    SequentialAnimation {
        running: root.blinks && !KodosiTheme.reduceMotion && KodosiTheme.shown && root.visible
        loops: Animation.Infinite
        alwaysRunToEnd: true
        PauseAnimation { duration: 520 }
        OpacityAnimator { target: block; from: 1; to: 0.2; duration: 90 }
        PauseAnimation { duration: 420 }
        OpacityAnimator { target: block; from: 0.2; to: 1; duration: 90 }
    }
}
