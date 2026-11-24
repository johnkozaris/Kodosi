pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

Column {
    id: root

    required property var step
    required property Models.TerminalChat chat
    required property Look look
    readonly property var detail: step.detail || ({})
    readonly property bool open: step.opens === true && chat.opened.indexOf(step.id) >= 0

    signal held()
    signal linkActivated(string link)

    spacing: 4

    AbstractButton {
        id: line

        width: root.width
        implicitHeight: 24
        hoverEnabled: true
        activeFocusOnTab: root.step.opens === true
        objectName: "chat.step." + root.step.id
        Accessible.role: root.step.opens ? Accessible.Button : Accessible.StaticText
        Accessible.name: root.step.words || ""
        ToolTip.visible: hovered && (root.step.target || "").length > 0
        ToolTip.text: root.step.target || ""
        ToolTip.delay: Tip.delay
        onClicked: {
            if (!root.step.opens)
                return
            root.held()
            root.chat.toggle(root.step.id)
        }

        background: Rectangle {
            radius: KodosiTheme.radiusXs
            color: "transparent"
            border.width: line.visualFocus ? 1.5 : 0
            border.color: root.look.accentStrong
        }
        contentItem: Item {
            Row {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 6

                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 15
                    height: 11
                    name: root.step.icon || "cube"
                    strokeWidth: 2.2
                    color: root.look.inkFaint
                }
                PlainLabel {
                    id: words

                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, root.width - 21 - stateMark.width - chevron.width - 18)
                    text: root.step.words || ""
                    color: root.step.open ? root.look.ink : root.look.inkMuted
                    font: root.step.mono ? Qt.font({ family: "monospace", pixelSize: KodosiTheme.fontCaption }) : Qt.font({ pixelSize: KodosiTheme.fontFootnote })
                    elide: Text.ElideMiddle
                }
                Item {
                    id: stateMark

                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(sign.visible ? sign.width : 0, stateWords.visible ? stateWords.implicitWidth : 0)
                    height: 15

                    StatusSign {
                        id: sign

                        anchors.verticalCenter: parent.verticalCenter
                        session: root.step.state === "done" || root.step.state === "failed"
                            ? { sign: root.step.state, asks: true, signLabel: root.step.state === "done" ? qsTr("Done") : qsTr("Failed") } : null
                        size: 10
                        look: root.look
                    }
                    PlainLabel {
                        id: stateWords

                        anchors.verticalCenter: parent.verticalCenter
                        visible: text.length > 0
                        text: root.step.state === "denied" ? qsTr("denied") : root.step.state === "stopped" ? qsTr("stopped") : ""
                        color: root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                }
                KIcon {
                    id: chevron

                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.step.opens === true
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
    Loader {
        x: 21
        width: root.width - 21
        active: root.open
        visible: active
        sourceComponent: root.detail.kind === "command" ? command
            : root.detail.kind === "change" ? change
            : root.detail.kind === "output" ? output
            : root.detail.kind === "question" ? question
            : root.detail.kind === "plan" ? plan
            : root.detail.kind === "agent" ? helper : null

        Component {
            id: command

            Column {
                bottomPadding: 6
                spacing: 6

                CodeWell {
                    width: parent.width
                    visible: root.detail.code === true
                    code: root.detail.command || ""
                    output: root.detail.output || ""
                    look: root.look
                }
                Row {
                    visible: root.detail.background === true
                    spacing: 5

                    KIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 11
                        height: 11
                        name: "refresh"
                        strokeWidth: 2.2
                        color: root.look.inkFaint
                    }
                    PlainLabel {
                        text: qsTr("in the background")
                        color: root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                }
            }
        }
        Component {
            id: change

            Column {
                bottomPadding: 6
                spacing: 4

                Row {
                    spacing: 6
                    ToolTip.visible: changeHover.hovered
                    ToolTip.text: root.detail.path || ""
                    ToolTip.delay: Tip.delay

                    HoverHandler { id: changeHover }
                    PlainLabel {
                        text: root.detail.name || ""
                        color: root.look.inkMuted
                        font.family: "monospace"
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                    PlainLabel {
                        text: "+" + (root.detail.added ?? 0)
                        color: root.look.ready
                        font.family: "monospace"
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                    PlainLabel {
                        text: "−" + (root.detail.removed ?? 0)
                        color: root.look.danger
                        font.family: "monospace"
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                }
                ChangeLines {
                    width: parent.width
                    lines: root.detail.lines || []
                    more: root.detail.more === true
                    look: root.look
                }
            }
        }
        Component {
            id: output

            Item {
                implicitHeight: well.implicitHeight + 6

                CodeWell {
                    id: well

                    width: parent.width
                    output: root.detail.text || ""
                    look: root.look
                }
            }
        }
        Component {
            id: question

            Column {
                bottomPadding: 6
                spacing: 6

                Repeater {
                    model: root.detail.pairs || []

                    Column {
                        id: pair

                        required property var modelData

                        width: parent.width
                        spacing: 2

                        KReadOnlyText {
                            width: parent.width
                            text: pair.modelData.question
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            selectionColor: root.look.accent
                        }
                        KReadOnlyText {
                            width: parent.width
                            visible: text.length > 0
                            text: pair.modelData.answer
                            color: root.look.ink
                            font.pixelSize: KodosiTheme.fontFootnote
                            font.weight: Font.DemiBold
                            selectionColor: root.look.accent
                        }
                    }
                }
            }
        }
        Component {
            id: plan

            Item {
                implicitHeight: planProse.implicitHeight + 30

                Raised {
                    width: parent.width
                    height: parent.height - 6
                    radius: KodosiTheme.radiusLg
                    look: root.look
                    fill: root.look.raised
                }
                AgentProse {
                    id: planProse

                    x: 12
                    y: 12
                    width: parent.width - 24
                    blocks: root.detail.blocks || []
                    look: root.look
                    onLinkActivated: link => root.linkActivated(link)
                }
            }
        }
        Component {
            id: helper

            Column {
                bottomPadding: 6
                spacing: 4

                Row {
                    spacing: 6

                    PlainLabel {
                        visible: text.length > 0
                        text: root.detail.name || ""
                        color: root.look.ink
                        font.pixelSize: KodosiTheme.fontFootnote
                        font.weight: Font.DemiBold
                    }
                    PlainLabel {
                        text: qsTr("%n steps", "", root.detail.steps ?? 0)
                        color: root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                }
                Row {
                    visible: (root.detail.doing || "").length > 0
                    width: parent.width
                    spacing: 6

                    KIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 15
                        height: 11
                        name: root.detail.doingIcon || "cube"
                        strokeWidth: 2.2
                        color: root.look.inkFaint
                    }
                    PlainLabel {
                        width: parent.width - 21
                        text: root.detail.doing || ""
                        color: root.look.inkMuted
                        font.family: "monospace"
                        font.pixelSize: KodosiTheme.fontCaption
                        elide: Text.ElideMiddle
                    }
                }
                Text {
                    width: parent.width
                    visible: (root.detail.said || "").length > 0
                    text: Prose.styled(root.detail.said || "", root.look)
                    textFormat: Text.RichText
                    wrapMode: Text.Wrap
                    maximumLineCount: 6
                    elide: Text.ElideRight
                    color: root.look.inkMuted
                    linkColor: root.look.accentStrong
                    font.pixelSize: KodosiTheme.fontFootnote
                    onLinkActivated: link => root.linkActivated(link)
                }
            }
        }
    }
}
