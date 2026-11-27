pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

// The mentions in `parts`, as Mentions.segments splits the words, show as chips behind the text.
TextEdit {
    id: root

    property var parts: []
    property string selfId: ""
    property Look look: KodosiTheme.look
    readonly property var chips: root.length > 0 && root.parts.some(part => part.target)
        ? Mentions.chips(root.parts, root.selfId, root.look, root.getText(0, root.length)) : []

    textFormat: TextEdit.RichText
    readOnly: true
    selectByMouse: true
    wrapMode: TextEdit.Wrap
    color: root.look.ink
    font.pixelSize: KodosiTheme.fontBody
    selectionColor: root.look.accent
    selectedTextColor: root.look.accentInk

    HoverHandler { cursorShape: root.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor }
    Repeater {
        model: root.chips

        Rectangle {
            required property var modelData
            readonly property bool laidOut: root.length > 0 && root.width > 0 && root.contentHeight > 0
            readonly property rect head: laidOut ? root.positionToRectangle(modelData.start) : Qt.rect(0, 0, 0, 0)
            readonly property rect tail: laidOut ? root.positionToRectangle(modelData.end) : Qt.rect(0, 0, 0, 0)

            z: -1
            x: head.x - 3
            y: head.y
            width: tail.x - head.x + 6
            height: head.height
            radius: 5
            visible: head.y === tail.y && tail.x > head.x
            color: KodosiTheme.alpha(modelData.color, 0.18)
        }
    }
}
