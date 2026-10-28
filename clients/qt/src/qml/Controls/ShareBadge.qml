import Kodosi 1.0
import QtQuick

Rectangle {
    id: root

    property string room: ""
    property color ring: KodosiTheme.ground

    width: 12
    height: 12
    radius: 4
    color: ring
    Accessible.ignored: true

    Rectangle {
        anchors.centerIn: parent
        width: 8
        height: 8
        radius: 2.5
        color: root.room.length > 0 ? Identity.roomTint(root.room) : KodosiTheme.inkFaint
    }
}
