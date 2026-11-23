pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

// The line is there from the first step, so that a person can open a step while it runs.
Column {
    id: root

    required property var line
    required property Models.TerminalChat chat
    required property Look look
    readonly property bool open: chat.opened.indexOf(line.id) >= 0

    signal held()

    AbstractButton {
        id: header

        width: Math.min(implicitWidth, root.width)
        implicitWidth: headerRow.implicitWidth
        implicitHeight: Math.max(26, headerRow.implicitHeight + 6)
        hoverEnabled: true
        activeFocusOnTab: true
        objectName: "chat.work." + root.line.id
        Accessible.role: Accessible.Button
        Accessible.name: [root.line.summary || ""].concat(root.line.setbacks || []).filter(words => words.length > 0).join(", ")
        Accessible.description: root.open ? qsTr("Hide the steps") : qsTr("Show the steps")
        ToolTip.visible: hovered
        ToolTip.text: Accessible.description
        ToolTip.delay: Tip.delay
        onClicked: {
            root.held()
            root.chat.toggle(root.line.id)
        }

        background: Rectangle {
            radius: KodosiTheme.radiusXs
            color: "transparent"
            border.width: header.visualFocus ? 1.5 : 0
            border.color: root.look.accentStrong
        }
        contentItem: Item {
            Row {
                id: headerRow

                anchors.verticalCenter: parent.verticalCenter
                spacing: 6

                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 15
                    height: 11
                    name: root.line.icon || "cube"
                    strokeWidth: 2.2
                    color: root.look.inkFaint
                }
                PlainLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, root.width - 96)
                    visible: text.length > 0
                    text: root.line.summary || ""
                    color: root.look.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                    wrapMode: Text.WordWrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                Repeater {
                    model: root.line.setbacks || []

                    PlainLabel {
                        required property string modelData

                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData
                        color: root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontFootnote
                    }
                }
                StatusSign {
                    anchors.verticalCenter: parent.verticalCenter
                    session: root.line.sign ? { sign: root.line.sign, asks: true, signLabel: root.line.sign === "done" ? qsTr("Done") : qsTr("Failed") } : null
                    size: 11
                    look: root.look
                }
                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 9
                    height: 9
                    name: "chevron-right"
                    rotation: root.open ? 90 : 0
                    strokeWidth: 2.8
                    color: root.look.inkFaint

                    Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                }
            }
        }
    }
    Column {
        width: root.width
        visible: root.open
        bottomPadding: 4

        Repeater {
            model: Models.KeyedList { values: root.open ? root.line.steps : [] }

            ChatStep {
                required property var modelData

                width: root.width
                step: modelData
                chat: root.chat
                look: root.look
                onHeld: root.held()
            }
        }
    }
}
