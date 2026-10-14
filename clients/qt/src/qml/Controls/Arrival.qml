import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

// A column that fades in and rises into place when it appears. With a key and a shared
// `seen` object, a column that a list creates again for a key it showed before stays still.
Item {
    id: root

    default property alias content: column.data
    property alias spacing: column.spacing
    property int delay: 0
    property real rise: 8
    property var seen: null
    property string key: ""
    property real arrival: 1

    function enter() {
        if (KodosiTheme.reduceMotion)
            return
        arrival = 0
        entrance.restart()
    }

    implicitWidth: column.implicitWidth
    implicitHeight: column.implicitHeight
    opacity: arrival
    transform: Translate { y: (1 - root.arrival) * root.rise }

    onVisibleChanged: {
        if (visible)
            enter()
    }
    Component.onCompleted: {
        const tracked = seen !== null && key.length > 0
        const known = tracked && seen[key] === true
        if (tracked)
            seen[key] = true
        if (!known)
            enter()
    }

    ColumnLayout {
        id: column
        anchors.fill: parent
    }
    SequentialAnimation {
        id: entrance

        PauseAnimation { duration: root.delay }
        NumberAnimation { target: root; property: "arrival"; to: 1; duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic }
    }
}
