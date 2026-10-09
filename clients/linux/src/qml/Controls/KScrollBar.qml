import Kodosi 1.0
import QtQuick
import QtQuick.Controls

ScrollBar {
    id: root

    property bool prominent: false

    implicitWidth: orientation === Qt.Vertical ? 9 : 80
    implicitHeight: orientation === Qt.Vertical ? 80 : 9
    padding: 2
    policy: ScrollBar.AsNeeded

    contentItem: Rectangle {
        implicitWidth: 5
        implicitHeight: 5
        radius: 2.5
        color: root.pressed ? KodosiTheme.accent : KodosiTheme.inkFaint
        opacity: root.prominent ? 0.8 : root.pressed || root.hovered ? 0.7 : root.active ? 0.45 : 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
    }
    background: Item {}
}
