import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var session: ({})
    property color color: KodosiTheme.inkMuted
    property int pixelSize: KodosiTheme.fontCaption
    readonly property string kind: session.placeKind || ""
    readonly property bool shown: kind.length > 0

    visible: shown
    implicitWidth: shown ? glyph.width + 4 + name.implicitWidth : 0
    implicitHeight: Math.max(glyph.height, name.implicitHeight)
    Accessible.role: Accessible.StaticText
    Accessible.name: session.placeLabel || ""
    ToolTip.visible: hover.hovered
    ToolTip.text: session.placeLabel || ""
    ToolTip.delay: Tip.delay

    HoverHandler { id: hover }
    KIcon {
        id: glyph
        anchors.verticalCenter: parent.verticalCenter
        width: root.pixelSize
        height: width
        name: root.kind === "container" ? "container" : root.kind === "vm" ? "cube" : "server"
        color: root.color
        strokeWidth: 2
    }
    PlainLabel {
        id: name
        x: glyph.width + 4
        width: Math.max(0, root.width - x)
        anchors.verticalCenter: parent.verticalCenter
        text: root.session.placeName || ""
        color: root.color
        font.pixelSize: root.pixelSize
        elide: Text.ElideRight
    }
}
