import Kodosi 1.0
import QtQuick
import QtQuick.Controls

ScrollBar {
    id: root

    property bool prominent: false

    implicitWidth: orientation === Qt.Vertical
        ? (prominent ? 10 : 8)
        : 80
    implicitHeight: orientation === Qt.Vertical
        ? 80
        : (prominent ? 10 : 8)
    padding: 2
    policy: ScrollBar.AsNeeded

    contentItem: Rectangle {
        implicitWidth: 4
        implicitHeight: 4
        radius: 2
        color: root.prominent
            ? KodosiTheme.textSecondary
            : root.pressed
            ? KodosiTheme.accent
            : root.hovered
              ? KodosiTheme.textTertiary
              : KodosiTheme.seamStrong
        opacity: root.prominent
            ? 1
            : root.active ? 0.62 : 0.28
    }

    background: Rectangle {
        color: root.prominent
            ? KodosiTheme.surface
            : KodosiTheme.surface
    }
}
