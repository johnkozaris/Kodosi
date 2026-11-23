import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic

AbstractButton {
    id: root

    property Look look: KodosiTheme.look
    property bool ready: false
    property bool busy: false

    implicitWidth: 30
    implicitHeight: 30
    enabled: ready
    hoverEnabled: true
    activeFocusOnTab: true
    scale: pressed ? 0.95 : 1
    Accessible.name: qsTr("Send")
    ToolTip.visible: hovered && ready
    ToolTip.text: Accessible.name
    ToolTip.delay: Tip.delay

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Item {
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: KodosiTheme.mix(root.look.lifted, root.look.ink, 0.08)
        }
        Bezel {
            anchors.fill: parent
            radius: width / 2
            look: root.look
            pressed: root.pressed
            fill: root.hovered ? root.look.accentHover : root.look.accent
            opacity: root.ready ? 1 : 0

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
        }
        Loader {
            anchors.fill: parent
            anchors.margins: -4
            opacity: root.busy ? 1 : 0
            active: opacity > 0

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

            sourceComponent: LightRim {}
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
        KIcon {
            anchors.centerIn: parent
            width: 14
            height: 14
            name: "send"
            strokeWidth: 2.6
            color: root.ready ? root.look.accentInk : root.look.inkFaint
        }
    }
}
