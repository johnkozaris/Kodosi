import Kodosi 1.0
import QtQuick

Rectangle {
    property bool active: false
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.margins: 4
    height: 2
    radius: 1
    color: KodosiTheme.focusRing
    visible: active
    Accessible.ignored: true
}
