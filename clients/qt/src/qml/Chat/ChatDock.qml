pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    required property Models.TerminalChat chat
    required property var session
    required property string sessionId
    required property Look look
    required property string agent
    property real room: 600
    readonly property real askRoom: room - 16 - composerArea.implicitHeight - factsRow.implicitHeight - 2 * dock.spacing
        - (tasks.visible ? tasks.implicitHeight + dock.spacing : 0)
    readonly property bool typed: chat.draft.trim().length > 0
    readonly property bool stops: chat.working && !typed
    readonly property bool away: chat.loaded && chat.state === "away"
    readonly property real column: Math.min(760, width - 20)

    signal sent()

    function focusComposer(): void {
        field.forceActiveFocus()
    }
    function send(): void {
        if (root.typed && root.chat.canWrite && root.chat.submit())
            root.sent()
    }

    implicitHeight: dock.implicitHeight + 12
    objectName: "chat.dock"

    ColumnLayout {
        id: dock

        x: (root.width - root.column) / 2
        y: 4
        width: root.column
        spacing: 8

        Item {
            id: tasks

            readonly property var list: root.chat.tasks
            readonly property int done: list.filter(task => task.state === "done").length
            readonly property var current: list.find(task => task.state === "working") || list.find(task => task.state === "pending") || null
            property bool open: false

            Layout.fillWidth: true
            implicitHeight: taskColumn.implicitHeight + 4
            visible: list.length > 0

            Bezel {
                anchors.fill: parent
                radius: 13
                quiet: true
                look: root.look
                fill: root.look.raised
            }
            Column {
                id: taskColumn

                x: 10
                y: 2
                width: parent.width - 20

                AbstractButton {
                    id: tasksKey

                    width: parent.width
                    implicitHeight: 22
                    hoverEnabled: true
                    activeFocusOnTab: true
                    objectName: "chat.tasks"
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Tasks: %1 of %2 done").arg(tasks.done).arg(tasks.list.length)
                    Accessible.description: tasks.open ? qsTr("Hide the tasks") : qsTr("Show the tasks")
                    ToolTip.visible: hovered
                    ToolTip.text: Accessible.description
                    ToolTip.delay: Tip.delay
                    onClicked: tasks.open = !tasks.open

                    background: Rectangle {
                        radius: KodosiTheme.radiusXs
                        color: "transparent"
                        border.width: tasksKey.visualFocus ? 1.5 : 0
                        border.color: root.look.accentStrong
                    }
                    contentItem: RowLayout {
                        spacing: 7

                        KIcon {
                            Layout.preferredWidth: 11
                            Layout.preferredHeight: 11
                            name: "tasks"
                            strokeWidth: 2.2
                            color: root.look.inkMuted
                        }
                        PlainLabel {
                            text: tasks.done + "/" + tasks.list.length
                            color: root.look.inkMuted
                            font.family: "monospace"
                            font.pixelSize: KodosiTheme.fontCaption
                        }
                        RollingText {
                            Layout.fillWidth: true
                            visible: !tasks.open && tasks.current !== null
                            text: tasks.current ? tasks.current.text : ""
                            color: root.look.ink
                            font.pixelSize: KodosiTheme.fontFootnote
                            elide: Text.ElideRight
                        }
                        Item {
                            Layout.fillWidth: true
                            visible: tasks.open || tasks.current === null
                        }
                        KIcon {
                            Layout.preferredWidth: 9
                            Layout.preferredHeight: 9
                            name: "chevron-up"
                            rotation: tasks.open ? 180 : 0
                            strokeWidth: 2.8
                            color: root.look.inkFaint

                            Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                        }
                    }
                }
                Column {
                    width: parent.width
                    visible: tasks.open
                    leftPadding: 2
                    bottomPadding: 4
                    spacing: 3

                    Repeater {
                        model: tasks.open ? tasks.list : []

                        Row {
                            id: task

                            required property var modelData

                            width: parent.width
                            spacing: 7
                            Accessible.role: Accessible.StaticText
                            Accessible.name: modelData.state === "done" ? qsTr("Done") + ", " + modelData.text : modelData.text

                            Item {
                                width: 12
                                height: 15

                                KIcon {
                                    anchors.centerIn: parent
                                    width: 10
                                    height: 10
                                    visible: task.modelData.state === "done"
                                    name: "check"
                                    strokeWidth: 3
                                    color: root.look.ready
                                }
                                Rectangle {
                                    anchors.centerIn: parent
                                    visible: task.modelData.state !== "done"
                                    width: 7
                                    height: 7
                                    radius: 3.5
                                    color: task.modelData.state === "working" ? root.look.ink : "transparent"
                                    border.width: task.modelData.state === "working" ? 0 : 1.2
                                    border.color: root.look.inkFaint
                                }
                            }
                            PlainLabel {
                                width: parent.width - 19
                                text: task.modelData.text
                                color: task.modelData.state === "working" ? root.look.ink : root.look.inkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                                wrapMode: Text.WordWrap
                            }
                        }
                    }
                }
            }
        }
        Repeater {
            model: Models.KeyedList { values: root.chat.queued }

            Item {
                id: waiting

                required property var modelData
                readonly property bool refused: (modelData.refused || "").length > 0

                Layout.fillWidth: true
                implicitHeight: queuedColumn.implicitHeight

                KIconButton {
                    anchors.right: queuedColumn.left
                    anchors.rightMargin: 6
                    anchors.verticalCenter: bubble.verticalCenter
                    glyph: "close"
                    size: 22
                    terminalLook: root.look
                    objectName: "chat.queued.remove." + waiting.modelData.id
                    Accessible.id: objectName
                    Accessible.name: qsTr("Remove")
                    onClicked: root.chat.unqueue(waiting.modelData.id)
                }
                Column {
                    id: queuedColumn

                    anchors.right: parent.right
                    width: Math.min(parent.width - 68, Math.max(queuedWords.implicitWidth + 40, refusedWords.visible ? refusedWords.implicitWidth : 0))
                    spacing: 3

                    Rectangle {
                        id: bubble

                        anchors.right: parent.right
                        width: Math.min(queuedWords.implicitWidth + 40, parent.width)
                        height: queuedWords.height + 16
                        radius: 16
                        bottomRightRadius: 6
                        color: root.look.accentSoft

                        KIcon {
                            x: 12
                            y: 11
                            width: 10
                            height: 10
                            name: "history"
                            strokeWidth: 2.4
                            color: root.look.inkFaint
                            Accessible.role: Accessible.StaticText
                            Accessible.name: qsTr("Waiting for %1").arg(root.agent)
                        }
                        PlainLabel {
                            id: queuedWords

                            x: 28
                            y: 8
                            width: Math.min(implicitWidth, bubble.parent.width - 40)
                            text: waiting.modelData.text
                            color: root.look.ink
                            font.pixelSize: KodosiTheme.fontFootnote
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                    }
                    PlainLabel {
                        id: refusedWords

                        anchors.right: parent.right
                        visible: waiting.refused
                        text: qsTr("The agent takes the next message in its terminal.")
                        color: root.look.caution
                        font.pixelSize: KodosiTheme.fontCaption
                        ToolTip.visible: refusedHover.hovered
                        ToolTip.text: waiting.modelData.refused || ""
                        ToolTip.delay: Tip.delay

                        HoverHandler { id: refusedHover }
                    }
                }
            }
        }
        Loader {
            Layout.fillWidth: true
            active: (root.chat.ask.id || "").length > 0
            visible: active
            sourceComponent: ChatAskCard {
                ask: root.chat.ask
                chat: root.chat
                session: root.session
                look: root.look
                room: root.askRoom
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 6
            visible: root.chat.failure.length > 0
            spacing: 7

            Rectangle {
                implicitWidth: 6
                implicitHeight: 6
                radius: 3
                color: root.look.caution
            }
            PlainLabel {
                Layout.fillWidth: true
                text: root.chat.failure
                color: root.look.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            Layout.topMargin: 10
            Layout.bottomMargin: 10
            visible: root.away
            text: qsTr("No agent runs in this terminal")
            color: root.look.inkFaint
            font.pixelSize: KodosiTheme.fontFootnote
            horizontalAlignment: Text.AlignHCenter
            objectName: "chat.away"
        }
        Item {
            id: composerArea

            Layout.fillWidth: true
            visible: !root.away
            implicitHeight: Math.min(176, Math.max(40, field.implicitHeight + 10))

            Behavior on implicitHeight { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

            ComposerBox {
                anchors.fill: parent
                look: root.look
                focused: field.activeFocus
            }
            TapHandler {
                onTapped: field.forceActiveFocus()
            }
            RowLayout {
                anchors.fill: parent
                anchors.margins: 5
                spacing: 4

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    TextArea {
                        id: field

                        placeholderText: root.chat.asking ? qsTr("Or say what to do instead") : qsTr("Message %1").arg(root.agent)
                        placeholderTextColor: root.look.inkFaint
                        color: root.look.ink
                        selectionColor: root.look.accent
                        selectedTextColor: root.look.accentInk
                        font.pixelSize: KodosiTheme.fontBody
                        wrapMode: TextEdit.Wrap
                        topPadding: 7
                        bottomPadding: 7
                        leftPadding: 10
                        rightPadding: 0
                        background: null
                        enabled: root.chat.canWrite
                        objectName: "chat.input"
                        Accessible.id: objectName
                        Accessible.name: placeholderText
                        Component.onCompleted: text = root.chat.draft
                        onTextChanged: root.chat.draft = text
                        Keys.onReturnPressed: event => {
                            if (event.modifiers & (Qt.ShiftModifier | Qt.AltModifier)) {
                                event.accepted = false
                                return
                            }
                            root.send()
                        }
                        Keys.onEnterPressed: event => {
                            if (event.modifiers & (Qt.ShiftModifier | Qt.AltModifier)) {
                                event.accepted = false
                                return
                            }
                            root.send()
                        }
                        Keys.onTabPressed: nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocusReason)
                        Keys.onBacktabPressed: nextItemInFocusChain(false).forceActiveFocus(Qt.BacktabFocusReason)

                        Connections {
                            function onDraftChanged() {
                                if (field.text !== root.chat.draft)
                                    field.text = root.chat.draft
                            }

                            target: root.chat
                        }
                    }
                }
                Item {
                    Layout.alignment: Qt.AlignBottom
                    implicitWidth: 30
                    implicitHeight: 30

                    StopKey {
                        anchors.fill: parent
                        visible: opacity > 0
                        opacity: root.stops ? 1 : 0
                        look: root.look
                        objectName: "chat.stop"
                        Accessible.id: objectName
                        onClicked: root.chat.stop()

                        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                    }
                    SendKey {
                        anchors.fill: parent
                        visible: opacity > 0
                        opacity: root.stops ? 0 : 1
                        ready: root.typed && root.chat.canWrite
                        look: root.look
                        objectName: "chat.send"
                        Accessible.id: objectName
                        onClicked: root.send()

                        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                    }
                }
            }
        }
        ChatFacts {
            id: factsRow

            Layout.fillWidth: true
            Layout.leftMargin: 6
            Layout.rightMargin: 6
            visible: !root.away
            facts: root.chat.facts
            session: root.session
            sessionId: root.sessionId
            look: root.look
        }
    }
}
