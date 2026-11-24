import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string html: ""
    property string text: ""
    property Look look: KodosiTheme.look
    readonly property real widest: Math.max(0, width - 56 - 24)

    signal linkActivated(string link)

    implicitHeight: bubble.height
    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Rectangle {
        id: bubble

        anchors.right: parent.right
        width: Math.min(words.implicitWidth, root.widest) + 24
        height: words.implicitHeight + 16
        radius: 16
        bottomRightRadius: 6
        color: root.look.accentSoft

        TextEdit {
            id: words

            x: 12
            y: 8
            width: Math.min(implicitWidth, root.widest)
            text: Prose.styled(root.html, root.look)
            textFormat: TextEdit.RichText
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            color: root.look.ink
            font.pixelSize: KodosiTheme.fontBody
            selectionColor: root.look.accent
            selectedTextColor: root.look.accentInk
            Accessible.ignored: true
            onLinkActivated: link => root.linkActivated(link)

            HoverHandler { cursorShape: words.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor }
        }
    }
}
