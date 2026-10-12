import Kodosi 1.0
import QtQuick

Item {
    id: root

    property var session: ({})
    property string words: session.activity || ""
    property color color: KodosiTheme.inkMuted
    property int pixelSize: KodosiTheme.fontFootnote
    readonly property string percent: session.progress >= 0 ? qsTr("%1%").arg(session.progress) : ""
    readonly property bool shown: words.length > 0 || percent.length > 0
    readonly property real gap: words.length > 0 && percent.length > 0 ? 5 : 0

    implicitWidth: (words.length > 0 ? line.implicitWidth : 0) + gap + (percent.length > 0 ? number.implicitWidth : 0)
    implicitHeight: Math.max(line.implicitHeight, number.implicitHeight)

    ShimmerText {
        id: line
        width: Math.min(implicitWidth, Math.max(0, root.width - root.gap - (root.percent.length > 0 ? number.implicitWidth : 0)))
        height: root.height
        visible: root.words.length > 0
        text: root.words
        active: root.session.working === true
        color: root.color
        font.pixelSize: root.pixelSize
        elide: Text.ElideRight
    }
    PlainLabel {
        id: number
        x: line.visible ? line.width + root.gap : 0
        height: root.height
        verticalAlignment: Text.AlignVCenter
        visible: root.percent.length > 0
        text: root.percent
        color: root.color
        font.pixelSize: root.pixelSize
        font.features: { "tnum": 1 }
    }
}
