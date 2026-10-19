import Kodosi 1.0
import QtQuick

Item {
    id: root

    required property string key
    required property string place
    property var session: null
    property string sigil: ""
    property color fill: "transparent"
    property real radius: 0
    property bool asleep: false
    property bool tucked: false
    property Item view: null
    readonly property bool hidden: Fold.hides(key, place)

    Accessible.ignored: true
    Component.onCompleted: Fold.enroll(root)
    Component.onDestruction: Fold.leave(root)
}
