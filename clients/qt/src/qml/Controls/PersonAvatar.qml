import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string name: ""
    property string key: ""
    property real size: 28
    property bool isSelf: false
    property color ring: "transparent"
    readonly property color tint: isSelf ? KodosiTheme.accent : Identity.personTint(key.length > 0 ? key : name)

    implicitWidth: size
    implicitHeight: size
    Accessible.role: Accessible.Graphic
    Accessible.name: name

    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        radius: width / 2
        color: root.ring
        visible: root.ring.a > 0
    }
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        gradient: Gradient {
            GradientStop { position: 0; color: KodosiTheme.mix(root.tint, "#ffffff", 0.2) }
            GradientStop { position: 0.5; color: root.tint }
            GradientStop { position: 1; color: KodosiTheme.mix(root.tint, "#000000", 0.18) }
        }
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.16)
    }
    Text {
        anchors.centerIn: parent
        text: Identity.initials(root.name)
        color: root.isSelf ? KodosiTheme.accentInk : "#ffffff"
        font.pixelSize: Math.round(root.size * 0.38)
        font.weight: Font.DemiBold
        textFormat: Text.PlainText
    }
}
