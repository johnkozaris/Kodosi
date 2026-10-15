import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string initial: ""
    property string key: ""
    property bool signedIn: false
    property real size: 26

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    PersonAvatar {
        anchors.fill: parent
        visible: root.signedIn
        name: root.initial
        key: root.key
        size: root.size
    }
    SlotMark {
        anchors.fill: parent
        visible: !root.signedIn
        text: root.initial
        tint: Identity.personTint(root.key)
        size: root.size
    }
}
