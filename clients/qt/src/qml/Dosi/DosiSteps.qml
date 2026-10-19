pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Column {
    id: root

    property var row: ({})
    property bool open: false
    readonly property bool group: row.kind === "tools"
    readonly property bool running: group ? row.running === true : !!row.tool && row.tool.running === true
    readonly property bool shown: group && (open || running)

    function reveal(tool) {
        if (tool.targetKind === "url")
            Models.DesktopFiles.openWebUrl(tool.targetValue)
        else
            Models.Dosi.open(tool.targetKind, tool.targetValue)
    }

    spacing: 2

    add: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionSnappy }
    }

    AbstractButton {
        id: header

        visible: root.group
        width: Math.min(implicitWidth, root.width)
        implicitWidth: headerRow.implicitWidth
        implicitHeight: 22
        hoverEnabled: true
        Accessible.name: root.row.summary || ""
        ToolTip.visible: hovered
        ToolTip.text: root.shown ? qsTr("Hide the steps") : qsTr("Show the steps")
        ToolTip.delay: 600
        onClicked: root.open = !root.open

        contentItem: Row {
            id: headerRow
            spacing: 7

            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: -4

                Repeater {
                    model: root.row.marks || []

                    Item {
                        id: mark

                        required property var modelData
                        required property int index

                        z: -index
                        width: mini.implicitWidth
                        height: 14

                        DosiMini {
                            id: mini
                            tool: mark.modelData
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: -1.5
                            radius: 5.5
                            color: "transparent"
                            border.width: 1.5
                            border.color: KodosiTheme.ground
                        }
                    }
                }
            }
            ShimmerText {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, root.width - 80)
                text: root.row.summary || ""
                active: root.running
                color: root.running ? KodosiTheme.ink : KodosiTheme.inkMuted
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideRight
            }
            Status {
                anchors.verticalCenter: parent.verticalCenter
                status: root.running ? "running" : root.row.failed ? "failed" : "done"
            }
            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 9
                height: 9
                name: "chevron-down"
                rotation: root.shown ? 0 : -90
                strokeWidth: 2.6
                color: KodosiTheme.inkFaint

                Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
            }
        }
    }
    Item {
        visible: root.shown
        width: root.width
        height: lines.implicitHeight

        Rectangle {
            x: 6
            width: 1
            height: parent.height
            color: KodosiTheme.lifted
        }
        Column {
            id: lines
            x: 18
            width: parent.width - 18

            add: Transition {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionSnappy }
                NumberAnimation { property: "y"; from: -3; duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic }
            }

            Repeater {
                model: root.shown ? root.row.tools : []

                Line {
                    required property var modelData
                    width: lines.width
                    tool: modelData
                    onReveal: root.reveal(modelData)
                }
            }
        }
    }
    Receipt {
        visible: !root.group && !!root.row.tool
        width: root.width
        tool: root.row.tool || ({})
        onReveal: root.reveal(tool)
    }

    component Status: Item {
        id: status

        property string status: "done"

        width: 11
        height: 11

        Spinner {
            anchors.centerIn: parent
            size: 10
            visible: status.status === "running"
        }
        KIcon {
            anchors.centerIn: parent
            width: 11
            height: 11
            visible: status.status === "done"
            name: "check"
            strokeWidth: 2.8
            color: KodosiTheme.ready
        }
        KIcon {
            anchors.centerIn: parent
            width: 11
            height: 11
            visible: status.status === "failed"
            name: "warning"
            strokeWidth: 2.4
            color: KodosiTheme.caution
        }
    }

    component Line: AbstractButton {
        id: line

        property var tool: ({})

        signal reveal

        implicitHeight: 22
        hoverEnabled: true
        Accessible.name: tool.receipt || ""
        ToolTip.visible: hovered && !!tool.targetValue
        ToolTip.text: tool.targetValue || ""
        ToolTip.delay: 600
        onClicked: {
            if (tool.openable)
                line.reveal()
        }

        contentItem: Item {
            DosiMini {
                id: lineMark
                anchors.verticalCenter: parent.verticalCenter
                tool: line.tool
            }
            PlainLabel {
                anchors.left: lineMark.right
                anchors.leftMargin: 7
                anchors.right: progress.left
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                text: line.tool.receipt || ""
                color: line.tool.running ? KodosiTheme.ink : KodosiTheme.inkMuted
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideMiddle
            }
            Status {
                id: progress
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                status: line.tool.status || "done"
            }
        }
    }

    component Receipt: Column {
        id: receipt

        property var tool: ({})
        property bool output: false

        signal reveal

        spacing: 6

        Row {
            spacing: 2

            AbstractButton {
                id: pill

                implicitHeight: 22
                implicitWidth: Math.min(pillRow.implicitWidth + 14, receipt.width - 24)
                hoverEnabled: true
                Accessible.name: receipt.tool.receipt || ""
                ToolTip.visible: hovered && !!receipt.tool.targetValue
                ToolTip.text: receipt.tool.targetValue || ""
                ToolTip.delay: 600
                scale: pressed ? 0.97 : 1
                onClicked: {
                    if (receipt.tool.openable)
                        receipt.reveal()
                    else if (receipt.tool.hasOutput)
                        receipt.output = !receipt.output
                }

                Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                background: Rectangle {
                    radius: height / 2
                    color: KodosiTheme.alpha(KodosiTheme.ink, pill.hovered ? 0.08 : KodosiTheme.isDark ? 0.045 : 0.05)
                }
                contentItem: Item {
                    Row {
                        id: pillRow
                        x: 5
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 7

                        DosiMini {
                            anchors.verticalCenter: parent.verticalCenter
                            tool: receipt.tool
                        }
                        ShimmerText {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, pill.width - 46)
                            text: receipt.tool.receipt || ""
                            active: receipt.tool.running === true
                            color: receipt.tool.running ? KodosiTheme.ink : KodosiTheme.inkMuted
                            font.family: "monospace"
                            font.pixelSize: KodosiTheme.fontCaption
                            elide: Text.ElideMiddle
                        }
                        Status {
                            anchors.verticalCenter: parent.verticalCenter
                            status: receipt.tool.status || "done"
                        }
                    }
                }
            }
            AbstractButton {
                visible: receipt.tool.hasOutput === true
                implicitWidth: 18
                implicitHeight: 22
                hoverEnabled: true
                Accessible.name: receipt.output ? qsTr("Hide the output") : qsTr("Show the output")
                ToolTip.visible: hovered
                ToolTip.text: Accessible.name
                ToolTip.delay: 600
                onClicked: receipt.output = !receipt.output

                contentItem: Item {
                    KIcon {
                        anchors.centerIn: parent
                        width: 9
                        height: 9
                        name: "chevron-down"
                        rotation: receipt.output ? 0 : -90
                        strokeWidth: 2.6
                        color: KodosiTheme.inkFaint

                        Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                    }
                }
            }
        }
        Item {
            visible: receipt.output
            width: receipt.width
            height: outputText.implicitHeight + 16

            Well {
                anchors.fill: parent
                radius: KodosiTheme.radiusMd
            }
            Text {
                id: outputText
                x: 8
                y: 8
                width: parent.width - 16
                text: receipt.tool.output || ""
                color: KodosiTheme.inkMuted
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 12
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }
        }
    }
}
