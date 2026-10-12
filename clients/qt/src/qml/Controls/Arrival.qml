import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

// A column that fades in and rises into place whenever it appears.
Item {
    id: root

    default property alias content: column.data
    property alias spacing: column.spacing
    property int delay: 0
    property real rise: 8
    property real arrival: KodosiTheme.reduceMotion ? 1 : 0

    implicitWidth: column.implicitWidth
    implicitHeight: column.implicitHeight
    opacity: arrival
    transform: Translate { y: (1 - root.arrival) * root.rise }

    onVisibleChanged: {
        if (visible && !KodosiTheme.reduceMotion) {
            arrival = 0
            entrance.restart()
        }
    }

    ColumnLayout {
        id: column
        anchors.fill: parent
    }
    SequentialAnimation {
        id: entrance
        running: !KodosiTheme.reduceMotion

        PauseAnimation { duration: root.delay }
        NumberAnimation { target: root; property: "arrival"; to: 1; duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic }
    }
}
