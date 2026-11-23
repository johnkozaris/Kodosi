import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic

AbstractButton {
    id: root

    property Look look: KodosiTheme.look

    implicitWidth: 30
    implicitHeight: 30
    hoverEnabled: true
    activeFocusOnTab: true
    scale: pressed ? 0.95 : 1
    Accessible.name: qsTr("Stop")
    ToolTip.visible: hovered
    ToolTip.text: Accessible.name
    ToolTip.delay: Tip.delay

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Item {
        Bezel {
            anchors.fill: parent
            radius: width / 2
            quiet: true
            look: root.look
            pressed: root.pressed
            fill: root.look.buttonFace(root.hovered)
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            radius: width / 2
            color: "transparent"
            border.width: root.visualFocus ? 1.5 : 0
            border.color: root.look.accentStrong
        }
    }
    contentItem: Item {
        Rectangle {
            anchors.centerIn: parent
            width: 9
            height: 9
            radius: 2
            color: root.look.ink
        }
    }
}
