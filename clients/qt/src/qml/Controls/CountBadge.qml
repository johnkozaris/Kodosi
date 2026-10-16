import Kodosi 1.0
import QtQuick

Rectangle {
    id: root

    property int count: 0

    visible: count > 0
    implicitWidth: Math.max(16, label.implicitWidth + 9)
    implicitHeight: 16
    radius: 8
    color: KodosiTheme.accent

    RollingText {
        id: label
        anchors.centerIn: parent
        text: root.count > 99 ? "99+" : String(root.count)
        color: KodosiTheme.accentInk
        font.pixelSize: KodosiTheme.fontCaption2
        font.weight: Font.Bold
    }
}
