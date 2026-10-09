import Kodosi 1.0
import QtQuick

Rectangle {
    id: root

    property int count: 0
    property bool quiet: false

    visible: count > 0
    implicitWidth: Math.max(16, label.implicitWidth + 9)
    implicitHeight: 16
    radius: 8
    color: quiet ? KodosiTheme.alpha(KodosiTheme.ink, 0.1) : KodosiTheme.accent

    Text {
        id: label
        anchors.centerIn: parent
        text: root.count > 99 ? "99+" : root.count
        color: root.quiet ? KodosiTheme.inkMuted : KodosiTheme.accentInk
        font.pixelSize: KodosiTheme.fontCaption2
        font.weight: Font.Bold
    }
}
