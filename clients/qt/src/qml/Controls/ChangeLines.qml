pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Item {
    id: root

    property var lines: []
    property bool more: false
    property Look look: KodosiTheme.look

    implicitHeight: column.implicitHeight + 20

    Well {
        anchors.fill: parent
        radius: KodosiTheme.radiusMd
        look: root.look
    }
    Column {
        id: column

        x: 10
        y: 10
        width: parent.width - 20

        Repeater {
            model: root.lines

            Row {
                id: line

                required property var modelData
                readonly property color tone: modelData.sign === "added" ? root.look.ready
                    : modelData.sign === "removed" ? root.look.danger : root.look.inkFaint

                width: column.width
                spacing: 6

                Text {
                    text: line.modelData.sign === "added" ? "+" : line.modelData.sign === "removed" ? "−" : " "
                    color: line.tone
                    font.family: "monospace"
                    font.pixelSize: KodosiTheme.fontCaption
                }
                Text {
                    width: line.width - 14
                    text: line.modelData.text
                    textFormat: Text.PlainText
                    color: line.tone
                    font.family: "monospace"
                    font.pixelSize: KodosiTheme.fontCaption
                    wrapMode: Text.WrapAnywhere
                    maximumLineCount: 4
                    elide: Text.ElideRight
                }
            }
        }
        Text {
            visible: root.more
            text: "…"
            color: root.look.inkFaint
            font.family: "monospace"
            font.pixelSize: KodosiTheme.fontCaption
        }
    }
}
