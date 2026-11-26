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
        - (tasks.visible ? tasks.implicitHeight + dock.spacing : 0) - (helpers.visible ? helpers.implicitHeight + dock.spacing : 0)
    readonly property bool typed: chat.draft.trim().length > 0
    readonly property bool stops: chat.working && !typed
    readonly property bool away: chat.loaded && chat.state === "away"
    readonly property real column: Math.min(760, width - 20)
    readonly property var suggestions: {
        const draft = chat.draft
        if (!draft.startsWith("/") || /\s/.test(draft))
            return []
        const letters = draft.slice(1).toLowerCase()
        const starting = chat.commands.filter(command => command.name.toLowerCase().startsWith(letters))
        if (letters.length === 0)
            return starting
        return starting.concat(chat.commands.filter(command => !command.name.toLowerCase().startsWith(letters)
            && command.name.toLowerCase().includes(letters)))
    }
    property int picked: 0

    signal sent()
    signal terminalRequested()
    signal linkActivated(string link)

    function focusComposer(): void {
        field.forceActiveFocus()
    }
    function send(): void {
        if (!root.typed || !root.chat.canWrite)
            return
        const control = root.chat.control(root.chat.draft)
        if (control.length > 0) {
            root.chat.draft = ""
            root.open(control)
        } else if (root.chat.submit()) {
            root.sent()
        }
    }
    function open(control: string): void {
        if (control === "model")
            factsRow.openModel()
        else if (control === "usage")
            factsRow.openUsage()
    }
    function move(step: int): bool {
        const count = root.suggestions.length
        if (count === 0)
            return false
        root.picked = (Math.min(root.picked, count - 1) + step + count) % count
        commandView.positionViewAtIndex(root.picked, ListView.Contain)
        return true
    }
    function complete(): bool {
        const count = root.suggestions.length
        if (count === 0)
            return false
        const command = root.suggestions[Math.min(root.picked, count - 1)]
        if (root.chat.draft === "/" + command.name)
            return false
        root.choose(command)
        return true
    }
    function choose(command: var): void {
        root.picked = 0
        if ((command.opens || "").length > 0) {
            root.chat.draft = ""
            root.open(command.opens)
        } else {
            root.chat.draft = "/" + command.name + " "
            field.cursorPosition = field.length
            field.forceActiveFocus()
        }
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
                    implicitHeight: KodosiTheme.scaled(22)
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
                            Layout.preferredWidth: KodosiTheme.scaled(11)
                            Layout.preferredHeight: KodosiTheme.scaled(11)
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
                            Layout.preferredWidth: KodosiTheme.scaled(9)
                            Layout.preferredHeight: KodosiTheme.scaled(9)
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
        Item {
            id: helpers

            readonly property var list: root.chat.helpers
            property bool open: true
            property var saying: []

            Layout.fillWidth: true
            implicitHeight: helperColumn.implicitHeight + 4
            visible: list.length > 0

            Bezel {
                anchors.fill: parent
                radius: 13
                quiet: true
                look: root.look
                fill: root.look.raised
            }
            Column {
                id: helperColumn

                x: 10
                y: 2
                width: parent.width - 20

                AbstractButton {
                    id: helpersKey

                    width: parent.width
                    implicitHeight: KodosiTheme.scaled(22)
                    hoverEnabled: true
                    activeFocusOnTab: true
                    objectName: "chat.helpers"
                    Accessible.role: Accessible.Button
                    Accessible.name: helpersWords.text
                    Accessible.description: helpers.open ? qsTr("Hide the helpers") : qsTr("Show the helpers")
                    ToolTip.visible: hovered
                    ToolTip.text: Accessible.description
                    ToolTip.delay: Tip.delay
                    onClicked: helpers.open = !helpers.open

                    background: Rectangle {
                        radius: KodosiTheme.radiusXs
                        color: "transparent"
                        border.width: helpersKey.visualFocus ? 1.5 : 0
                        border.color: root.look.accentStrong
                    }
                    contentItem: RowLayout {
                        spacing: 7

                        KIcon {
                            Layout.preferredWidth: KodosiTheme.scaled(11)
                            Layout.preferredHeight: KodosiTheme.scaled(11)
                            name: "people"
                            strokeWidth: 2.2
                            color: root.look.inkMuted
                        }
                        PlainLabel {
                            id: helpersWords

                            Layout.fillWidth: true
                            text: qsTr("%n helpers work", "", helpers.list.length)
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            elide: Text.ElideRight
                        }
                        KIcon {
                            Layout.preferredWidth: KodosiTheme.scaled(9)
                            Layout.preferredHeight: KodosiTheme.scaled(9)
                            name: "chevron-up"
                            rotation: helpers.open ? 180 : 0
                            strokeWidth: 2.8
                            color: root.look.inkFaint

                            Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                        }
                    }
                }
                Column {
                    width: parent.width
                    visible: helpers.open
                    leftPadding: 2
                    bottomPadding: 6
                    spacing: 8

                    Repeater {
                        model: Models.KeyedList { values: helpers.open ? helpers.list : [] }

                        AbstractButton {
                            id: helper

                            required property var modelData
                            readonly property string kind: modelData.kind || ""
                            readonly property string task: modelData.task || ""
                            readonly property int steps: modelData.steps ?? 0
                            readonly property var doing: modelData.doing || null
                            readonly property string said: modelData.said || ""
                            readonly property bool says: helpers.saying.includes(modelData.id)
                            readonly property string stepWords: qsTr("%n steps", "", steps)

                            width: parent.width - 2
                            implicitHeight: helperRow.implicitHeight
                            hoverEnabled: true
                            activeFocusOnTab: true
                            objectName: "chat.helper." + modelData.id
                            Accessible.role: Accessible.Button
                            Accessible.name: [kind.length > 0 && task.length > 0 ? kind + " · " + task : kind + task, doing ? doing.target : "",
                                doing || steps > 0 ? stepWords : "", says ? said : ""].filter(part => part.length > 0).join(", ")
                            Accessible.description: says ? qsTr("Hide what the helper said") : qsTr("Show what the helper said")
                            ToolTip.visible: hovered
                            ToolTip.text: Accessible.description
                            ToolTip.delay: Tip.delay
                            onClicked: helpers.saying = says ? helpers.saying.filter(key => key !== modelData.id) : helpers.saying.concat([modelData.id])

                            background: Rectangle {
                                anchors.fill: parent
                                anchors.margins: -3
                                radius: KodosiTheme.radiusXs
                                color: "transparent"
                                border.width: helper.visualFocus ? 1.5 : 0
                                border.color: root.look.accentStrong
                            }
                            contentItem: Row {
                                id: helperRow

                                spacing: 8

                                AgentMark {
                                    size: 18
                                    program: root.session.program || ""
                                    account: root.session.programAccount ?? -1
                                    working: true
                                }
                                Column {
                                    width: helper.width - 26
                                    spacing: 2

                                    Row {
                                        width: parent.width

                                        PlainLabel {
                                            id: kindWords

                                            width: Math.min(implicitWidth, parent.width)
                                            visible: helper.kind.length > 0
                                            text: helper.kind
                                            color: root.look.ink
                                            font.pixelSize: KodosiTheme.fontFootnote
                                            font.weight: Font.DemiBold
                                            elide: Text.ElideRight
                                        }
                                        PlainLabel {
                                            width: Math.max(0, parent.width - (kindWords.visible ? kindWords.width : 0))
                                            visible: helper.task.length > 0
                                            text: helper.kind.length > 0 ? " · " + helper.task : helper.task
                                            color: helper.kind.length > 0 ? root.look.inkMuted : root.look.ink
                                            font.pixelSize: KodosiTheme.fontFootnote
                                            elide: Text.ElideRight
                                        }
                                    }
                                    Row {
                                        width: parent.width
                                        visible: helper.doing !== null || helper.steps > 0
                                        spacing: 6

                                        KIcon {
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: helper.doing !== null
                                            width: KodosiTheme.scaled(15)
                                            height: KodosiTheme.scaled(11)
                                            name: helper.doing ? helper.doing.icon || "cube" : "cube"
                                            strokeWidth: 2.2
                                            color: root.look.inkFaint
                                        }
                                        PlainLabel {
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: helper.doing !== null
                                            width: Math.max(0, Math.min(implicitWidth, parent.width - 27 - stepsWords.implicitWidth))
                                            text: helper.doing ? helper.doing.target : ""
                                            color: root.look.inkMuted
                                            font.family: "monospace"
                                            font.pixelSize: KodosiTheme.fontCaption
                                            elide: Text.ElideMiddle
                                        }
                                        PlainLabel {
                                            id: stepsWords

                                            anchors.verticalCenter: parent.verticalCenter
                                            text: helper.stepWords
                                            color: root.look.inkFaint
                                            font.pixelSize: KodosiTheme.fontCaption
                                            font.features: { "tnum": 1 }
                                        }
                                    }
                                    Text {
                                        width: parent.width
                                        visible: helper.says && helper.said.length > 0
                                        topPadding: 2
                                        text: helper.says ? Prose.styled(Models.Markdown.text(helper.said), root.look) : ""
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
                }
            }
        }
        Repeater {
            model: Models.KeyedList { values: root.chat.queued }

            Item {
                id: waiting

                required property var modelData
                readonly property bool refused: (modelData.refused || "").length > 0
                readonly property bool terminal: !refused && modelData.terminal === true
                readonly property bool given: !refused && !terminal && modelData.given === true
                readonly property bool removable: !terminal && !given

                Layout.fillWidth: true
                implicitHeight: queuedColumn.implicitHeight

                KIconButton {
                    visible: waiting.removable
                    anchors.right: queuedColumn.left
                    anchors.rightMargin: 6
                    y: (bubble.height - height) / 2
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
                    width: Math.min(parent.width - (waiting.removable ? 68 : 40), Math.max(queuedWords.implicitWidth + 40, refusedWords.visible ? refusedWords.implicitWidth : 0,
                        terminalWords.visible ? terminalText.implicitWidth + terminalWords.spacing + showTerminal.implicitWidth : 0))
                    spacing: 3

                    Rectangle {
                        id: bubble

                        anchors.right: parent.right
                        width: Math.min(queuedWords.implicitWidth + 40, parent.width)
                        height: queuedWords.height + 16
                        radius: 16
                        bottomRightRadius: 6
                        color: root.look.accentSoft
                        ToolTip.visible: bubbleHover.hovered
                        ToolTip.text: waitingSign.Accessible.name
                        ToolTip.delay: Tip.delay

                        HoverHandler { id: bubbleHover }

                        KIcon {
                            id: waitingSign

                            x: 12
                            y: 11
                            width: 10
                            height: 10
                            name: waiting.given ? "check" : "history"
                            strokeWidth: 2.4
                            color: root.look.inkFaint
                            Accessible.role: Accessible.StaticText
                            Accessible.name: waiting.given ? qsTr("%1 reads this before its next step.").arg(root.agent)
                                : qsTr("Waiting for %1").arg(root.agent)
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
                        width: Math.min(implicitWidth, parent.width)
                        visible: waiting.refused
                        text: qsTr("The agent takes the next message in its terminal.")
                        wrapMode: Text.WordWrap
                        horizontalAlignment: Text.AlignRight
                        color: root.look.caution
                        font.pixelSize: KodosiTheme.fontCaption
                        ToolTip.visible: refusedHover.hovered
                        ToolTip.text: waiting.modelData.refused || ""
                        ToolTip.delay: Tip.delay

                        HoverHandler { id: refusedHover }
                    }
                    Row {
                        id: terminalWords

                        anchors.right: parent.right
                        visible: waiting.terminal
                        spacing: 8

                        PlainLabel {
                            id: terminalText

                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, queuedColumn.width - terminalWords.spacing - showTerminal.implicitWidth)
                            text: qsTr("%1 shows this in its terminal.").arg(root.agent)
                            wrapMode: Text.WordWrap
                            horizontalAlignment: Text.AlignRight
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                        }
                        AbstractButton {
                            id: showTerminal

                            anchors.verticalCenter: parent.verticalCenter
                            implicitWidth: showWords.implicitWidth
                            implicitHeight: KodosiTheme.scaled(22)
                            hoverEnabled: true
                            activeFocusOnTab: true
                            objectName: "chat.queued.terminal"
                            Accessible.role: Accessible.Button
                            Accessible.name: showWords.text
                            onClicked: root.terminalRequested()

                            background: Rectangle {
                                anchors.fill: parent
                                anchors.margins: -3
                                radius: KodosiTheme.radiusXs
                                color: "transparent"
                                border.width: showTerminal.visualFocus ? 1.5 : 0
                                border.color: root.look.accentStrong
                            }
                            contentItem: PlainLabel {
                                id: showWords

                                text: qsTr("Show the terminal")
                                color: showTerminal.hovered ? root.look.accent : root.look.accentStrong
                                font.pixelSize: KodosiTheme.fontCaption
                                font.weight: Font.DemiBold
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
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
                onLinkActivated: link => root.linkActivated(link)
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
            id: commandList

            readonly property real rowHeight: 30

            Layout.fillWidth: true
            implicitHeight: Math.min(commandView.contentHeight, rowHeight * 6.5) + 8
            visible: root.suggestions.length > 0 && !root.away && root.chat.canWrite
            objectName: "chat.commands"
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Commands")

            Raised {
                anchors.fill: parent
                radius: KodosiTheme.radiusLg
                look: root.look
                fill: root.look.raised
            }
            ListView {
                id: commandView

                anchors.fill: parent
                anchors.margins: 4
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.suggestions
                ScrollBar.vertical: KScrollBar {}

                delegate: AbstractButton {
                    id: commandRow

                    required property var modelData
                    required property int index

                    width: ListView.view.width
                    implicitHeight: commandList.rowHeight
                    hoverEnabled: true
                    focusPolicy: Qt.NoFocus
                    objectName: "chat.command." + modelData.name
                    Accessible.role: Accessible.Button
                    Accessible.name: "/" + modelData.name
                    Accessible.description: modelData.description || ""
                    onHoveredChanged: {
                        if (hovered)
                            root.picked = index
                    }
                    onClicked: root.choose(modelData)

                    background: Rectangle {
                        radius: KodosiTheme.radiusSm
                        color: commandRow.index === Math.min(root.picked, root.suggestions.length - 1)
                            ? KodosiTheme.alpha(root.look.ink, 0.07) : "transparent"
                    }
                    contentItem: Item {
                        Row {
                            x: 10
                            width: parent.width - 20
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 10

                            PlainLabel {
                                id: commandName

                                text: "/" + commandRow.modelData.name
                                color: root.look.ink
                                font.family: "monospace"
                                font.pixelSize: KodosiTheme.fontFootnote
                            }
                            PlainLabel {
                                anchors.baseline: commandName.baseline
                                width: Math.max(0, parent.width - commandName.implicitWidth - 10)
                                text: commandRow.modelData.description || ""
                                color: root.look.inkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
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
                            if (!root.complete())
                                root.send()
                        }
                        Keys.onEnterPressed: event => {
                            if (event.modifiers & (Qt.ShiftModifier | Qt.AltModifier)) {
                                event.accepted = false
                                return
                            }
                            if (!root.complete())
                                root.send()
                        }
                        Keys.onUpPressed: event => {
                            event.accepted = root.move(-1)
                        }
                        Keys.onDownPressed: event => {
                            event.accepted = root.move(1)
                        }
                        Keys.onTabPressed: {
                            if (!root.complete())
                                nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocusReason)
                        }
                        Keys.onBacktabPressed: nextItemInFocusChain(false).forceActiveFocus(Qt.BacktabFocusReason)

                        Connections {
                            function onDraftChanged() {
                                root.picked = 0
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
