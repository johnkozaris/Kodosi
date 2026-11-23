pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// With little room, what the answer is about gets less of it, so that the keys stay.
Item {
    id: root

    required property var ask
    required property Models.TerminalChat chat
    required property var session
    required property Look look
    property real room: 600
    readonly property string given: (chat.answers[ask.id] || {}).key || ""

    implicitHeight: content.implicitHeight + 22
    objectName: "chat.ask." + ask.id
    Accessible.role: Accessible.Grouping
    Accessible.name: ask.kind === "permission" ? ask.title || "" : ask.kind === "plan" ? qsTr("Plan") : ask.topics || ""

    Raised {
        anchors.fill: parent
        radius: KodosiTheme.radiusLg
        look: root.look
        fill: root.look.raised
        elevation: 2
    }
    Loader {
        id: content

        x: 12
        y: 11
        width: parent.width - 24
        sourceComponent: root.ask.kind === "permission" ? permission : root.ask.kind === "question" ? question : plan
    }

    component Header: RowLayout {
        id: header

        property string sign: "hand"
        property var session: ({})
        property Look look: KodosiTheme.look
        default property alias title: holder.data

        spacing: 8

        AgentMark {
            size: 20
            program: header.session.program || ""
            account: header.session.programAccount ?? -1
            working: false
        }
        RowLayout {
            id: holder

            Layout.fillWidth: true
            spacing: 6
        }
        StatusSign {
            session: ({ sign: header.sign, asks: true, signLabel: header.sign === "question" ? qsTr("Needs your answer") : qsTr("Needs your approval") })
            size: 14
            look: header.look
        }
    }
    component AnswerKey: KButton {
        id: key

        required property Models.TerminalChat chat
        required property string askId
        required property string given
        property string answer: ""
        property var payload: ({ kind: answer })
        readonly property bool chosen: given.length > 0 && given === answer

        compact: true
        enabled: given.length === 0 || chosen
        scale: chosen || pressed ? 0.97 : 1
        Accessible.checked: chosen
        onClicked: {
            if (given.length === 0)
                chat.answer(askId, payload)
        }
    }

    Component {
        id: permission

        ColumnLayout {
            id: permissionCard

            readonly property real detailRoom: root.room - 22 - 24 - keys.implicitHeight - 20
                - (helperRow.visible ? helperRow.implicitHeight + 10 : 0)
            readonly property real commandRoom: detailRoom >= 150 ? 140 : detailRoom >= 66 ? 56 : 0
            readonly property int lineRoom: detailRoom >= 16 * changeLine.height + 30 ? 14
                : detailRoom >= 6 * changeLine.height + 30 ? 4 : 0

            spacing: 10

            FontMetrics {
                id: changeLine

                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
            }
            Header {
                Layout.fillWidth: true
                sign: "hand"
                session: root.session
                look: root.look

                KIcon {
                    Layout.preferredWidth: 15
                    Layout.preferredHeight: 11
                    Layout.alignment: Qt.AlignVCenter
                    name: root.ask.icon || "cube"
                    strokeWidth: 2.2
                    color: root.look.inkFaint
                }
                PlainLabel {
                    readonly property bool whole: (root.ask.whole || "").length > 0 && permissionCard.commandRoom > 0

                    Layout.fillWidth: true
                    text: root.ask.title || ""
                    color: root.look.ink
                    font.family: "monospace"
                    font.pixelSize: KodosiTheme.fontFootnote
                    wrapMode: whole ? Text.NoWrap : Text.WrapAnywhere
                    maximumLineCount: whole ? 1 : 3
                    elide: whole ? Text.ElideMiddle : Text.ElideRight
                    ToolTip.visible: titleHover.hovered
                    ToolTip.text: root.ask.name || ""
                    ToolTip.delay: Tip.delay

                    HoverHandler { id: titleHover }
                }
            }
            RowLayout {
                id: helperRow

                visible: (root.ask.helper || "").length > 0
                spacing: 5

                KIcon {
                    Layout.preferredWidth: 11
                    Layout.preferredHeight: 11
                    name: "people"
                    strokeWidth: 2.2
                    color: root.look.inkFaint
                }
                PlainLabel {
                    Layout.fillWidth: true
                    text: root.ask.helper || ""
                    color: root.look.inkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                    elide: Text.ElideRight
                }
            }
            CodeWell {
                Layout.fillWidth: true
                visible: (root.ask.whole || "").length > 0 && permissionCard.commandRoom > 0
                code: root.ask.whole || ""
                most: permissionCard.commandRoom
                look: root.look
            }
            ChangeLines {
                Layout.fillWidth: true
                visible: (root.ask.lines || []).length > 0 && permissionCard.lineRoom > 0
                lines: (root.ask.lines || []).slice(0, permissionCard.lineRoom)
                more: (root.ask.lines || []).length > permissionCard.lineRoom
                look: root.look
            }
            Item {
                id: keys

                readonly property bool together: allow.implicitWidth + deny.implicitWidth + always.implicitWidth + 18 <= width

                Layout.fillWidth: true
                implicitHeight: together || !always.visible ? allow.implicitHeight : allow.implicitHeight * 2 + 6

                AnswerKey {
                    id: allow

                    chat: root.chat
                    askId: root.ask.id
                    given: root.given
                    terminalLook: root.look
                    answer: "allow"
                    variant: KButton.Primary
                    text: qsTr("Allow")
                    objectName: "chat.ask.allow"
                }
                AnswerKey {
                    id: deny

                    x: allow.width + 6
                    chat: root.chat
                    askId: root.ask.id
                    given: root.given
                    terminalLook: root.look
                    answer: "deny"
                    text: qsTr("Deny")
                    objectName: "chat.ask.deny"
                }
                AnswerKey {
                    id: always

                    x: keys.together ? keys.width - width : 0
                    y: keys.together ? 0 : allow.height + 6
                    width: Math.min(implicitWidth, keys.width)
                    visible: (root.ask.always || "").length > 0
                    chat: root.chat
                    askId: root.ask.id
                    given: root.given
                    terminalLook: root.look
                    answer: "always"
                    text: qsTr("Always %1").arg(root.ask.always || "")
                    objectName: "chat.ask.always"
                    ToolTip.visible: hovered
                    ToolTip.text: root.ask.always || ""
                    ToolTip.delay: Tip.delay

                    contentItem: Text {
                        text: qsTr("Always %1").arg(root.ask.alwaysCode
                            ? "<span style=\"font-family:monospace;font-weight:normal;color:" + root.look.inkMuted + "\">"
                                + Prose.escaped(root.ask.always || "") + "</span>"
                            : Prose.escaped(root.ask.always || ""))
                        textFormat: Text.RichText
                        elide: Text.ElideMiddle
                        color: always.foreground
                        font.pixelSize: KodosiTheme.fontFootnote
                        font.weight: Font.DemiBold
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
    }

    Component {
        id: question

        ColumnLayout {
            id: card

            property var picked: ({})
            readonly property var sent: (root.chat.answers[root.ask.id] || {}).answers || null
            readonly property var questions: root.ask.questions || []
            readonly property bool complete: questions.every((entry, index) => (picked[index] || []).length > 0)

            function choose(label: string, index: int): void {
                if (root.ask.direct) {
                    root.chat.answer(root.ask.id, { kind: "answers", answers: [[label]] })
                    return
                }
                const next = Object.assign({}, picked)
                let chosen = (next[index] || []).slice()
                if (questions[index].multiple) {
                    const at = chosen.indexOf(label)
                    if (at >= 0)
                        chosen.splice(at, 1)
                    else
                        chosen.push(label)
                } else {
                    chosen = [label]
                }
                next[index] = chosen
                picked = next
            }

            spacing: 10

            Header {
                Layout.fillWidth: true
                sign: "question"
                session: root.session
                look: root.look

                PlainLabel {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    text: root.ask.topics || ""
                    color: root.look.ink
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
            }
            Flickable {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(list.implicitHeight, Math.max(120, root.room - 110))
                contentHeight: list.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                clip: contentHeight > height
                ScrollBar.vertical: KScrollBar {}

                Column {
                    id: list

                    width: parent.width
                    spacing: 14

                    Repeater {
                        model: card.questions

                        Column {
                            id: asked

                            required property var modelData
                            required property int index
                            readonly property var chosen: card.sent ? card.sent[index] || [] : card.picked[index] || []

                            width: list.width
                            spacing: 8

                            KReadOnlyText {
                                width: parent.width
                                text: asked.modelData.question
                                color: root.look.ink
                                selectionColor: root.look.accent
                            }
                            Item {
                                width: parent.width
                                implicitHeight: options.implicitHeight + 6

                                Well {
                                    anchors.fill: parent
                                    radius: KodosiTheme.radiusMd
                                    look: root.look
                                }
                                Column {
                                    id: options

                                    x: 3
                                    y: 3
                                    width: parent.width - 6
                                    spacing: 2

                                    Repeater {
                                        model: asked.modelData.options

                                        AbstractButton {
                                            id: option

                                            required property var modelData
                                            readonly property bool on: asked.chosen.indexOf(modelData.label) >= 0

                                            width: options.width
                                            implicitHeight: Math.max(28, optionText.implicitHeight + 14)
                                            hoverEnabled: true
                                            activeFocusOnTab: true
                                            enabled: !card.sent || on
                                            scale: pressed ? 0.98 : 1
                                            objectName: "chat.ask.option." + modelData.label
                                            Accessible.role: asked.modelData.multiple ? Accessible.CheckBox : Accessible.RadioButton
                                            Accessible.name: modelData.label
                                            Accessible.description: modelData.description
                                            Accessible.checked: on
                                            onClicked: {
                                                if (!card.sent)
                                                    card.choose(modelData.label, asked.index)
                                            }

                                            Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                                            background: Item {
                                                Raised {
                                                    anchors.fill: parent
                                                    radius: KodosiTheme.radiusMd - 3
                                                    look: root.look
                                                    fill: root.look.raised
                                                    opacity: option.on ? 1 : 0
                                                    visible: opacity > 0

                                                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                                                }
                                                Rectangle {
                                                    anchors.fill: parent
                                                    radius: KodosiTheme.radiusMd - 3
                                                    color: KodosiTheme.alpha(root.look.ink, option.hovered && !option.on && !card.sent ? 0.05 : 0)
                                                    border.width: option.visualFocus ? 1.5 : 0
                                                    border.color: root.look.accentStrong
                                                }
                                            }
                                            contentItem: Item {
                                                Column {
                                                    id: optionText

                                                    x: 11
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    width: parent.width - 22 - (asked.modelData.multiple ? 20 : 0)
                                                    spacing: 2

                                                    PlainLabel {
                                                        width: parent.width
                                                        text: option.modelData.label
                                                        color: option.on ? root.look.ink : root.look.inkMuted
                                                        font.weight: Font.DemiBold
                                                        wrapMode: Text.WordWrap
                                                    }
                                                    PlainLabel {
                                                        width: parent.width
                                                        visible: text.length > 0
                                                        text: option.modelData.description
                                                        color: root.look.inkFaint
                                                        font.pixelSize: KodosiTheme.fontFootnote
                                                        wrapMode: Text.WordWrap
                                                        maximumLineCount: 3
                                                        elide: Text.ElideRight
                                                    }
                                                }
                                                Rectangle {
                                                    anchors.right: parent.right
                                                    anchors.rightMargin: 9
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    visible: asked.modelData.multiple
                                                    width: 14
                                                    height: 14
                                                    radius: 7
                                                    color: option.on ? root.look.accent : "transparent"
                                                    border.width: option.on ? 0 : 1.5
                                                    border.color: root.look.inkFaint

                                                    KIcon {
                                                        anchors.centerIn: parent
                                                        width: 10
                                                        height: 10
                                                        visible: option.on
                                                        name: "check"
                                                        strokeWidth: 3
                                                        color: root.look.accentInk
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            KButton {
                visible: !root.ask.direct
                compact: true
                variant: KButton.Primary
                terminalLook: root.look
                text: qsTr("Send")
                enabled: !card.sent && card.complete
                objectName: "chat.ask.send"
                onClicked: root.chat.answer(root.ask.id, { kind: "answers", answers: card.questions.map((entry, index) => card.picked[index] || []) })
            }
        }
    }

    Component {
        id: plan

        ColumnLayout {
            spacing: 10

            Header {
                Layout.fillWidth: true
                sign: "hand"
                session: root.session
                look: root.look

                KIcon {
                    Layout.preferredWidth: 15
                    Layout.preferredHeight: 11
                    name: "tasks"
                    strokeWidth: 2.2
                    color: root.look.inkFaint
                }
                PlainLabel {
                    Layout.fillWidth: true
                    text: qsTr("Plan")
                    color: root.look.ink
                    font.weight: Font.DemiBold
                }
            }
            Item {
                readonly property real most: Math.min(260, Math.max(0, root.room - 130))

                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(prose.implicitHeight + 24, most)
                visible: most > 40 && (root.ask.blocks || []).length > 0

                Rectangle {
                    anchors.fill: parent
                    radius: KodosiTheme.radiusMd
                    color: root.look.surface
                }
                Flickable {
                    anchors.fill: parent
                    contentHeight: prose.implicitHeight + 24
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true
                    ScrollBar.vertical: KScrollBar {}

                    AgentProse {
                        id: prose

                        x: 12
                        y: 12
                        width: parent.width - 24
                        blocks: root.ask.blocks || []
                        look: root.look
                    }
                }
            }
            Flow {
                Layout.fillWidth: true
                spacing: 6

                Repeater {
                    model: root.ask.choices || []

                    AnswerKey {
                        required property var modelData
                        required property int index

                        chat: root.chat
                        askId: root.ask.id
                        given: root.given
                        terminalLook: root.look
                        answer: "plan." + modelData.choice
                        payload: ({ kind: "plan", choice: modelData.choice })
                        variant: index === 0 ? KButton.Primary : KButton.Secondary
                        text: modelData.label
                        objectName: "chat.ask.plan." + modelData.choice
                    }
                }
                AnswerKey {
                    chat: root.chat
                    askId: root.ask.id
                    given: root.given
                    terminalLook: root.look
                    answer: "deny"
                    text: qsTr("Keep planning")
                    objectName: "chat.ask.deny"
                }
            }
        }
    }
}
