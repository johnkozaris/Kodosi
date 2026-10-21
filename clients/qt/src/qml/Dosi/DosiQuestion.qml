pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property var row: ({})
    property var picked: []
    readonly property bool answered: row.open === false
    readonly property var session: row.sessionId ? Models.Sessions.presentationForSession(row.sessionId) : null
    readonly property string words: own.text.trim()
    readonly property var choices: (row.options || []).filter(option => picked.indexOf(option) >= 0)
        .concat(words.length > 0 ? [words] : [])

    function choose(option) {
        if (!row.multiple) {
            Models.Dosi.answer(row.id, [option])
            return
        }
        const next = picked.slice()
        const index = next.indexOf(option)
        if (index >= 0)
            next.splice(index, 1)
        else
            next.push(option)
        picked = next
    }
    function send() {
        if (choices.length > 0 && (row.multiple || choices.length === 1))
            Models.Dosi.answer(row.id, choices)
    }

    implicitHeight: answered ? pill.height : card.height
    Accessible.role: Accessible.Grouping
    Accessible.name: row.question || ""

    Behavior on implicitHeight { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }

    Item {
        id: pill

        width: Math.min(pillRow.implicitWidth + 18, root.width)
        height: 22
        opacity: root.answered ? 1 : 0
        scale: root.answered ? 1 : 0.9
        visible: opacity > 0
        transformOrigin: Item.Left

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: KodosiTheme.surface
        }
        Row {
            id: pillRow
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 7

            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 10
                height: 10
                name: root.row.skipped ? "minus" : "check"
                strokeWidth: 2.8
                color: root.row.skipped ? KodosiTheme.inkFaint : KodosiTheme.ready
            }
            PlainLabel {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, root.width - 40)
                text: root.row.answered || ""
                color: KodosiTheme.inkMuted
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideRight
            }
        }
        HoverHandler { id: pillHover }
        ToolTip.visible: pillHover.hovered
        ToolTip.text: root.row.question || ""
        ToolTip.delay: Tip.delay
    }
    Item {
        id: card

        width: root.width
        height: content.implicitHeight + 22
        opacity: root.answered ? 0 : 1
        scale: root.answered ? 0.96 : 1
        visible: opacity > 0
        transformOrigin: Item.TopLeft

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }

        Raised {
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
            fill: KodosiTheme.raised
            elevation: 2
        }
        Column {
            id: content
            x: 12
            y: 11
            width: parent.width - 24
            spacing: 9

            Row {
                width: parent.width
                spacing: 8

                AgentMark {
                    id: mark
                    visible: !!root.session && !!root.session.id
                    anchors.verticalCenter: parent.verticalCenter
                    size: 18
                    program: root.session ? root.session.program || "" : ""
                    session: root.session
                }
                PlainLabel {
                    width: parent.width - (mark.visible ? 26 : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.row.question || ""
                    color: KodosiTheme.ink
                    font.pixelSize: KodosiTheme.fontFootnote
                    font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                }
            }
            Flow {
                width: parent.width
                spacing: 6

                Repeater {
                    model: root.row.options || []

                    KButton {
                        required property string modelData

                        compact: true
                        text: modelData
                        variant: root.row.multiple && root.picked.indexOf(modelData) >= 0 ? KButton.Tinted : KButton.Secondary
                        objectName: "dosi.answer." + modelData
                        Accessible.id: objectName
                        onClicked: root.choose(modelData)
                    }
                }
            }
            Row {
                width: parent.width
                spacing: 6

                KTextField {
                    id: own

                    compact: true
                    width: parent.width - (send.shown ? send.width + parent.spacing : 0)
                    placeholderText: qsTr("Something else")
                    objectName: "dosi.answer.own"
                    Accessible.id: objectName
                    Accessible.name: placeholderText
                    onAccepted: root.send()

                    Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                }
                KButton {
                    id: send

                    readonly property bool shown: root.row.multiple === true || root.words.length > 0

                    visible: opacity > 0
                    opacity: shown ? 1 : 0
                    scale: shown ? 1 : 0.9
                    compact: true
                    variant: KButton.Primary
                    text: qsTr("Send")
                    enabled: root.choices.length > 0
                    objectName: "dosi.answer.send"
                    Accessible.id: objectName
                    onClicked: root.send()

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                }
            }
        }
    }
}
