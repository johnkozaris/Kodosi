import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string iconName: ""
    property color tint: KodosiTheme.accent
    property real size: 28

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Rectangle {
        anchors.fill: parent
        radius: root.size * 0.3
        gradient: Gradient {
            GradientStop { position: 0; color: KodosiTheme.mix(root.tint, "#ffffff", 0.22) }
            GradientStop { position: 0.5; color: root.tint }
            GradientStop { position: 1; color: KodosiTheme.mix(root.tint, "#000000", 0.2) }
        }
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.18)
    }
    KIcon {
        anchors.centerIn: parent
        width: root.size * 0.56
        height: width
        name: root.iconName
        color: "#ffffff"
        strokeWidth: 2
    }
}
