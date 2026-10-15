import Kodosi 1.0
import QtQuick

Rectangle {
    property color ring: "transparent"

    implicitWidth: 8
    implicitHeight: 8
    radius: width / 2
    color: KodosiTheme.accent
    border.width: ring.a > 0 ? 1.5 : 0
    border.color: ring
}
