import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

AbstractButton {
    id: root

    property string agent: ""
    property real size: 16
    property bool onTerminal: false

    implicitWidth: size + 10
    implicitHeight: size + 10
    hoverEnabled: true
    scale: pressed ? 0.95 : 1
    Accessible.name: qsTr("Start %1").arg(Models.DesktopSettings.agentLabel(agent))
    ToolTip.visible: hovered && enabled
    ToolTip.delay: Tip.delay
    ToolTip.text: Accessible.name

    background: Rectangle {
        radius: KodosiTheme.radiusSm
        color: KodosiTheme.alpha(root.onTerminal ? KodosiTheme.terminalInk : KodosiTheme.ink, root.hovered && root.enabled ? 0.1 : 0)
    }
    contentItem: Item {
        AgentMark {
            anchors.centerIn: parent
            program: root.agent
            size: root.size
        }
    }
}
