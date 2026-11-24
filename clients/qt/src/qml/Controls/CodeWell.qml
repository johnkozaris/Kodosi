import Kodosi 1.0
import QtQuick

// What a command printed starts at its end, where a result or a fault is.
Item {
    id: root

    property string code: ""
    property string output: ""
    property real most: 180
    property Look look: KodosiTheme.look

    implicitHeight: Math.min(most, column.implicitHeight + 20)

    Well {
        anchors.fill: parent
        radius: KodosiTheme.radiusMd
        look: root.look
    }
    Flickable {
        id: view

        anchors.fill: parent
        anchors.margins: 10
        contentWidth: width
        contentHeight: column.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true

        onContentHeightChanged: if (root.output.length > 0) contentY = Math.max(0, contentHeight - height)
        onHeightChanged: if (root.output.length > 0) contentY = Math.max(0, contentHeight - height)

        Column {
            id: column

            width: view.width
            spacing: 6

            TextEdit {
                width: parent.width
                visible: root.code.length > 0
                text: root.code
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                color: root.look.ink
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                selectionColor: root.look.accent
                selectedTextColor: root.look.accentInk
            }
            TextEdit {
                width: parent.width
                visible: root.output.length > 0
                text: root.output
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                color: root.look.inkMuted
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                selectionColor: root.look.accent
                selectedTextColor: root.look.accentInk
            }
        }
    }
}
