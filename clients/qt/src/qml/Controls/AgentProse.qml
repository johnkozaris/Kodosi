pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Column {
    id: root

    property var blocks: []
    property bool live: false
    property Look look: KodosiTheme.look

    signal linkActivated(string link)

    spacing: 9
    Accessible.role: Accessible.StaticText

    Models.KeyedList {
        id: shown
        key: "key"
        values: root.blocks
    }
    Repeater {
        model: shown

        delegate: Loader {
            id: block

            required property var modelData
            required property int index
            readonly property bool last: index === shown.count - 1

            width: root.width
            sourceComponent: modelData.kind === "heading" ? heading
                : modelData.kind === "code" ? code
                : modelData.kind === "table" ? table : lines

            Component {
                id: lines

                Item {
                    implicitHeight: Math.max(words.implicitHeight, 18)

                    TextEdit {
                        id: words

                        width: Math.min(implicitWidth, parent.width - (block.last && root.live ? 12 : 0))
                        text: Prose.styled(block.modelData.html || "", root.look)
                        textFormat: TextEdit.RichText
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        color: root.look.ink
                        font.pixelSize: KodosiTheme.fontBody
                        selectionColor: root.look.accent
                        selectedTextColor: root.look.accentInk
                        Accessible.name: getText(0, length)
                        onLinkActivated: link => root.linkActivated(link)

                        HoverHandler { cursorShape: words.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor }
                    }
                    CursorBlock {
                        readonly property rect end: words.positionToRectangle(words.length)

                        x: Math.min(end.x + 3, parent.width - width)
                        y: end.y + Math.max(0, (end.height - height) / 2)
                        width: 6
                        height: 14
                        blinks: visible
                        color: root.look.accent
                        visible: block.last && root.live
                    }
                }
            }
            Component {
                id: heading

                TextEdit {
                    topPadding: block.index === 0 ? 0 : 3
                    text: Prose.styled(block.modelData.html || "", root.look)
                    textFormat: TextEdit.RichText
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    color: root.look.ink
                    font.pixelSize: KodosiTheme.fontBody
                    font.weight: Font.DemiBold
                    selectionColor: root.look.accent
                    selectedTextColor: root.look.accentInk
                    Accessible.role: Accessible.Heading
                    Accessible.name: getText(0, length)
                    onLinkActivated: link => root.linkActivated(link)
                }
            }
            Component {
                id: code

                Item {
                    implicitHeight: codeText.implicitHeight + 16

                    Well {
                        anchors.fill: parent
                        radius: KodosiTheme.radiusMd
                        look: root.look
                    }
                    Flickable {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        anchors.rightMargin: 10
                        contentWidth: codeText.implicitWidth
                        contentHeight: height
                        flickableDirection: Flickable.HorizontalFlick
                        boundsBehavior: Flickable.StopAtBounds
                        clip: true

                        TextEdit {
                            id: codeText

                            y: 8
                            text: block.modelData.text || ""
                            textFormat: TextEdit.PlainText
                            readOnly: true
                            selectByMouse: true
                            color: root.look.ink
                            font.family: "monospace"
                            font.pixelSize: KodosiTheme.fontCaption
                            selectionColor: root.look.accent
                            selectedTextColor: root.look.accentInk
                        }
                    }
                }
            }
            Component {
                id: table

                Flickable {
                    implicitHeight: grid.implicitHeight + 4
                    contentWidth: grid.implicitWidth
                    contentHeight: height
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true

                    GridLayout {
                        id: grid

                        readonly property var header: block.modelData.header || []
                        readonly property var sides: block.modelData.sides || []
                        readonly property int columnCount: block.modelData.columns || 1

                        y: 2
                        columns: columnCount
                        columnSpacing: 18
                        rowSpacing: 7

                        Repeater {
                            model: grid.header

                            Cell {
                                required property var modelData
                                required property int index

                                html: modelData
                                side: grid.sides[index] || "left"
                                bold: true
                                look: root.look
                                onLinkActivated: link => root.linkActivated(link)
                            }
                        }
                        Rectangle {
                            visible: grid.header.length > 0
                            Layout.columnSpan: grid.columnCount
                            Layout.fillWidth: true
                            implicitHeight: 1
                            color: root.look.hairline
                        }
                        Repeater {
                            model: (block.modelData.rows || []).reduce((cells, row) => cells.concat(row), [])

                            Cell {
                                required property var modelData
                                required property int index

                                html: modelData
                                side: grid.sides[index % grid.columnCount] || "left"
                                look: root.look
                                onLinkActivated: link => root.linkActivated(link)
                            }
                        }
                    }
                }
            }
        }
    }

    component Cell: Text {
        property string html: ""
        property string side: "left"
        property bool bold: false
        property Look look: KodosiTheme.look

        Layout.maximumWidth: 280
        Layout.alignment: (side === "right" ? Qt.AlignRight : side === "center" ? Qt.AlignHCenter : Qt.AlignLeft) | Qt.AlignTop
        text: Prose.styled(html, look)
        textFormat: Text.RichText
        wrapMode: Text.Wrap
        horizontalAlignment: side === "right" ? Text.AlignRight : side === "center" ? Text.AlignHCenter : Text.AlignLeft
        color: look.ink
        linkColor: look.accentStrong
        font.pixelSize: KodosiTheme.fontBody
        font.weight: bold ? Font.DemiBold : Font.Normal
    }
}
