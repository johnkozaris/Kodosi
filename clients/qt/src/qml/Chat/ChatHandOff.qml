import Kodosi 1.0
import QtQuick

Item {
    id: root

    required property var handoff
    required property Look look
    required property real widest
    property int pixelSize: KodosiTheme.fontBody
    property real markSize: 18
    readonly property string name: handoff ? handoff.name : ""
    readonly property string words: qsTr("Hand-off from %1").arg(name)

    implicitWidth: mark.width + 7 + label.width
    implicitHeight: Math.max(mark.height, label.implicitHeight)
    Accessible.role: Accessible.StaticText
    Accessible.name: words

    AgentMark {
        id: mark

        anchors.verticalCenter: parent.verticalCenter
        program: root.handoff ? root.handoff.agent : ""
        size: root.markSize
    }
    PlainLabel {
        id: label

        x: mark.width + 7
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, Math.max(0, root.widest - x))
        text: Prose.escaped(qsTr("Hand-off from %1")).arg("<span style=\"font-weight:600\">" + Prose.escaped(root.name) + "</span>")
        textFormat: Text.RichText
        color: root.look.ink
        font.pixelSize: root.pixelSize
        wrapMode: Text.Wrap
        Accessible.ignored: true
    }
}
