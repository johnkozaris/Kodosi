import Kodosi 1.0
import QtQuick
import QtQuick.Controls

AbstractButton {
    id: root

    property var command: ({})
    property real size: 16
    property bool onTerminal: false
    readonly property string agent: command.agent || "shell"

    implicitWidth: size + 10
    implicitHeight: size + 10
    hoverEnabled: true
    scale: pressed ? 0.9 : 1
    Accessible.name: qsTr("Start %1").arg(command.name || "")
    ToolTip.visible: hovered && enabled
    ToolTip.text: Accessible.name

    background: Rectangle {
        radius: KodosiTheme.radiusSm
        color: KodosiTheme.alpha(root.onTerminal ? KodosiTheme.terminalInk : KodosiTheme.ink, root.hovered && root.enabled ? 0.1 : 0)
    }
    contentItem: Item {
        AgentMark {
            anchors.centerIn: parent
            visible: root.command.repeated !== true
            program: root.agent === "shell" ? "" : root.agent
            size: root.size
        }
        Rectangle {
            anchors.centerIn: parent
            visible: root.command.repeated === true
            width: root.size
            height: root.size
            radius: root.size * 0.3
            color: Identity.kindTint(root.agent)

            PlainLabel {
                anchors.centerIn: parent
                text: root.command.initial || ""
                color: "#ffffff"
                font.pixelSize: Math.round(root.size * 0.6)
                font.weight: Font.Bold
            }
        }
    }
}
