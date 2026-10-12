import Kodosi 1.0
import QtQuick

Rectangle {
    property bool active: false

    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.leftMargin: Math.min(12, parent.width / 4)
    anchors.rightMargin: Math.min(12, parent.width / 4)
    anchors.bottomMargin: 3
    height: 2
    radius: 1
    color: KodosiTheme.accentStrong
    visible: active
    Accessible.ignored: true
}
