import Kodosi 1.0
import QtQuick

// An icon set into the surface, above the one line that says what is going on.
Well {
    id: root

    property string iconName: ""

    implicitWidth: 44
    implicitHeight: 44
    radius: 13
    Accessible.ignored: true

    KIcon {
        anchors.centerIn: parent
        width: 20
        height: 20
        name: root.iconName
        color: root.look.inkMuted
        strokeWidth: 1.7
    }
}
