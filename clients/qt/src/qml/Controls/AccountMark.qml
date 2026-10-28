import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string initial: ""
    property string key: ""
    property bool signedIn: false
    property real size: 26
    property var usage: null
    property bool onTerminal: false
    readonly property real ring: Math.max(1.5, size * 0.075)
    readonly property real face: usage ? size - 2 * (ring + Math.max(1.25, size * 0.06)) : size

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    PersonAvatar {
        anchors.centerIn: parent
        width: root.face
        height: root.face
        visible: root.signedIn
        name: root.initial
        key: root.key
        size: root.face
    }
    SlotMark {
        anchors.centerIn: parent
        width: root.face
        height: root.face
        visible: !root.signedIn
        text: root.initial
        tint: Identity.personTint(root.key)
        size: root.face
    }
    UsageRing {
        anchors.fill: parent
        visible: root.usage !== null
        limit: root.usage
        lineWidth: root.ring
        onTerminal: root.onTerminal
    }
}
