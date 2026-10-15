pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    property bool welcome: false
    property string folder: ""
    property bool showsAll: false
    property string menuKind: ""
    property bool adding: false
    readonly property font commandFont: Qt.font({ family: "monospace", pixelSize: KodosiTheme.fontBody, weight: Font.Medium })
    readonly property string home: Models.DesktopSettings.homeDirectory
    readonly property var choice: Models.Launcher.choice
    readonly property var agents: Models.Launcher.agents
    readonly property string agentId: choice.agent || ""
    readonly property var info: agents.find(entry => entry.agent === agentId) || null
    readonly property var modelSet: info ? Models.Launcher.models[info.agent] : undefined
    readonly property bool loadingModels: info !== null && Models.Launcher.loadingModels.indexOf(info.agent) >= 0
    readonly property var account: {
        if (!info)
            return null
        const saved = (choice.accounts || {})[info.agent]
        return info.accounts.find(entry => entry.command === saved) || info.accounts[0] || null
    }
    readonly property string modelArgument: {
        const saved = info ? (choice.models || {})[info.agent] || "" : ""
        if (!saved || !modelSet)
            return saved
        return modelSet.models.some(entry => entry.argument === saved) ? saved : ""
    }
    readonly property var model: modelSet ? modelSet.models.find(entry => entry.argument === modelArgument) || null : null
    readonly property var efforts: {
        const all = modelSet ? modelSet.efforts : []
        return !model || model.efforts.length === 0 ? all : all.filter(entry => model.efforts.indexOf(entry.id) >= 0)
    }
    readonly property string effortArgument: {
        const saved = info ? (choice.efforts || {})[info.agent] || "" : ""
        if (!saved || !modelSet)
            return saved
        return efforts.some(entry => entry.argument === saved) ? saved : ""
    }
    readonly property string directory: folder || Models.DesktopSettings.effectiveWorkingDirectory
    readonly property string line: !info ? "" : !info.installed ? info.install
        : !account ? "" : [account.command, modelArgument, effortArgument].filter(word => word.length > 0).join(" ")
    readonly property string agentLabel: info ? Models.DesktopSettings.agentLabel(info.agent) : ""
    readonly property string folderTitle: tilde(directory) === "~" ? "~" : Models.Sessions.folderName(directory)
    readonly property string accountWord: !info || !account ? "" : account.command === account.name ? account.name : info.program
    readonly property string modelTitle: model ? model.label : modelArgument.length > 0 ? modelArgument.split(" ").pop() : ""
    readonly property int effortLevel: efforts.findIndex(entry => entry.argument === effortArgument)
    readonly property bool takesWords: !info || info.installed
    readonly property bool listsModels: !modelSet || modelSet.models.length > 0
    readonly property bool ready: !info ? agentId.length === 0 : line.length > 0
    readonly property var hints: !info ? ["git status", "npm run dev", "make test", "ls -la"]
        : [qsTr("fix the failing test"), qsTr("explain how this folder works"), qsTr("review my last commit"),
            qsTr("add a dark mode"), qsTr("write tests for the parser")]
    readonly property string suggestion: hints[hint % hints.length]
    property bool opened: false
    property string ghost: ""
    property int hint: Math.floor(Math.random() * 20)

    signal resumeRequested

    function tilde(path) {
        return path === home ? "~" : path.startsWith(home + "/") ? "~" + path.slice(home.length) : path
    }
    function start() {
        if (!ready)
            return
        const words = promptInput.text.trim()
        let command = line
        let first = ""
        if (!info)
            command = words
        else if (info.installed && words.length > 0) {
            command = [line, info.promptFlag || ""].filter(word => word.length > 0).join(" ")
            first = words
        }
        if (Models.SessionActions.start("", directory, command, first))
            promptInput.clear()
    }
    function typeHints() {
        typist.stop()
        ghost = ""
        if (!takesWords || promptInput.length > 0)
            return
        if (KodosiTheme.reduceMotion) {
            ghost = suggestion
            return
        }
        typist.interval = 1200
        typist.start()
    }
    function typeStep() {
        if (ghost.length >= suggestion.length)
            return
        ghost = suggestion.slice(0, ghost.length + 1)
        typist.interval = ghost.endsWith(" ") ? 120 : 38 + Math.floor(Math.random() * 52)
        typist.start()
    }
    function accountTitle(entry) {
        if (!info || !entry)
            return ""
        if (entry.name === info.program)
            return qsTr("Default")
        let base = entry.name
        for (const separator of ["-", "_"]) {
            if (base.length > info.program.length + 1 && base.startsWith(info.program + separator)) {
                base = base.slice(info.program.length + 1)
                break
            }
        }
        return base.charAt(0).toUpperCase() + base.slice(1)
    }
    function accountRows() {
        if (!info)
            return []
        const rows = info.accounts.map(entry => {
            const title = accountTitle(entry)
            return { id: entry.command, kind: "choice", title: title, detail: entry.options || "",
                subtitle: entry.identity || (entry.folder ? qsTr("Signs in when it starts") : qsTr("Your shell chooses the sign-in")),
                icon: "account", initial: title.charAt(0), key: entry.folder || entry.command, signedIn: !!entry.identity,
                current: account !== null && entry.command === account.command }
        })
        if (info.addsAccounts)
            rows.push({ id: "menu.add", kind: "action", title: qsTr("Add an account"),
                subtitle: qsTr("Another subscription with its own sign-in"), icon: "slot", glyph: "plus" })
        return rows
    }
    function modelRows() {
        const rows = [{ id: "menu.default", kind: "choice", title: qsTr("Default"), current: modelArgument.length === 0 }]
        if (!modelSet)
            rows.push({ id: "menu.models", kind: "note", working: loadingModels,
                title: loadingModels ? qsTr("Asking %1 for its models").arg(agentLabel) : qsTr("%1 lists no models").arg(agentLabel) })
        for (const entry of modelSet ? modelSet.models : [])
            rows.push({ id: entry.argument, kind: "choice", title: entry.label, current: entry.argument === modelArgument,
                detail: entry.label.toLowerCase() === entry.id.toLowerCase() ? "" : entry.id })
        return rows
    }
    function effortLevels() {
        return efforts.map(entry => ({ id: entry.id, argument: entry.argument,
            label: entry.id === "xhigh" ? qsTr("Extra high") : entry.id.charAt(0).toUpperCase() + entry.id.slice(1) }))
    }
    function folderRows() {
        const seen = []
        const local = Models.Sessions.sessions.filter(session => session.kind === "local").map(session => session.workingDirectory || "")
        for (const path of [folder, Models.DesktopSettings.effectiveWorkingDirectory].concat(local).concat([home])) {
            if (path && seen.indexOf(path) < 0 && seen.length < 6)
                seen.push(path)
        }
        return seen.map(path => ({ id: path, kind: "choice", icon: "folder", subtitle: tilde(path), current: path === directory,
            title: tilde(path) === "~" ? "~" : Models.Sessions.folderName(path) }))
            .concat([{ id: "menu.choose", kind: "action", title: qsTr("Choose a folder…"), icon: "slot", glyph: "folder" }])
    }
    function popover(kind) {
        return kind === "folder" ? folderPopover : kind === "account" ? accountPopover : modelPopover
    }
    function toggleMenu(kind) {
        if (menuKind === kind) {
            closeMenu()
            return
        }
        adding = false
        menuKind = kind
        popover(kind).open()
    }
    function closeMenu() {
        menuKind = ""
        adding = false
        for (const item of [folderPopover, accountPopover, modelPopover]) {
            if (item.visible)
                item.close()
        }
        promptInput.forceActiveFocus()
    }
    function pick(kind, id) {
        if (kind === "folder") {
            if (id === "menu.choose") {
                closeMenu()
                Models.DesktopFiles.requestDirectory("launcher", directory)
                return
            }
            folder = id
        } else if (kind === "account") {
            if (id === "menu.add") {
                adding = true
                accountName.text = ""
                accountName.forceActiveFocus()
                return
            }
            Models.Launcher.choose("accounts", info.agent, id)
        } else if (kind === "model") {
            Models.Launcher.choose("models", info.agent, id === "menu.default" ? "" : id)
        } else if (kind === "effort") {
            Models.Launcher.choose("efforts", info.agent, id)
        }
        closeMenu()
    }

    spacing: welcome ? 24 : 16
    objectName: "launcher"
    Accessible.id: objectName

    function grow() {
        if (opened || growth.running || activation.enabled || !visible)
            return
        if (!welcome || Models.Launcher.grown || KodosiTheme.reduceMotion) {
            preview.seed = 1
            opened = true
            promptInput.forceActiveFocus()
            return
        }
        Models.Launcher.grown = true
        // A window that does not show yet keeps the cursor alone until it is active.
        if (root.Window.active)
            growth.start()
        else
            activation.enabled = true
    }

    Connections {
        id: activation

        target: root.Window.window
        enabled: false

        function onActiveChanged() {
            if (!root.Window.active)
                return
            activation.enabled = false
            growth.start()
        }
    }

    Component.onCompleted: {
        Models.Launcher.refresh(false)
        typeHints()
        grow()
    }
    onVisibleChanged: {
        grow()
        if (visible && opened)
            promptInput.forceActiveFocus()
    }
    onAgentIdChanged: {
        closeMenu()
        Models.Launcher.loadModels(agentId)
        typeHints()
        promptInput.forceActiveFocus()
    }
    onTakesWordsChanged: typeHints()
    onInfoChanged: Models.Launcher.loadModels(agentId)

    Timer {
        id: typist

        onTriggered: root.typeStep()
    }

    Connections {
        target: Models.Launcher

        function onAccountAdded(agent, command) {
            root.closeMenu()
        }
    }
    Connections {
        target: Models.DesktopFiles

        function onDirectoryPicked(purpose, path) {
            if (purpose === "launcher")
                root.folder = path
        }
    }
    Connections {
        target: Qt.application

        function onStateChanged() {
            if (Qt.application.state === Qt.ApplicationActive)
                Models.Launcher.refresh(false)
        }
    }

    component TypedText: Text {
        id: typed

        property string target: ""
        property string shown: ""

        text: shown
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
        Accessible.name: target
        onTargetChanged: step.restart()
        Component.onCompleted: step.restart()

        Timer {
            id: step
            interval: 14
            repeat: true
            triggeredOnStart: true
            onTriggered: {
                const target = typed.target
                const shown = typed.shown
                if (KodosiTheme.reduceMotion) {
                    typed.shown = target
                    stop()
                    return
                }
                let kept = 0
                while (kept < shown.length && kept < target.length && shown[kept] === target[kept])
                    kept++
                if (shown.length > kept)
                    typed.shown = shown.slice(0, -1)
                else if (shown.length < target.length)
                    typed.shown = target.slice(0, shown.length + 1)
                else
                    stop()
            }
        }
    }

    Item {
        id: preview

        property real seed: 0
        readonly property rect cursor: Qt.rect(16, band.height + 18, 9, 18)
        readonly property real wide: Math.min(1, Math.max(0, seed / 0.45))
        readonly property real tall: Math.min(1, Math.max(0, (seed - 0.45) / 0.55))

        Layout.alignment: root.welcome ? Qt.AlignHCenter : Qt.AlignLeft
        readonly property real bodyHeight: Math.max(root.welcome ? 124 : 92, terminalBody.implicitHeight + 32)

        Layout.preferredWidth: Math.min(560, root.width)
        implicitHeight: band.height + bodyHeight + deck.height
        objectName: "launcher.preview"
        Accessible.id: objectName

        SequentialAnimation {
            id: growth
            PauseAnimation { duration: 600 }
            NumberAnimation { target: preview; property: "seed"; to: 1; duration: 950; easing.type: Easing.OutCubic }
            ScriptAction {
                script: {
                    root.opened = true
                    promptInput.forceActiveFocus()
                }
            }
        }

        Item {
            id: seedClip

            readonly property real lineX: preview.cursor.x + (-24 - preview.cursor.x) * preview.wide
            readonly property real lineWidth: preview.cursor.width + (preview.width + 48 - preview.cursor.width) * preview.wide

            x: lineX
            y: preview.cursor.y + (-24 - preview.cursor.y) * preview.tall
            width: lineWidth
            height: preview.cursor.height + (preview.height + 48 - preview.cursor.height) * preview.tall
            clip: preview.seed < 1

            Item {
                x: -seedClip.x
                y: -seedClip.y
                width: preview.width
                height: preview.height

                Raised {
                    anchors.fill: parent
                    radius: KodosiTheme.radiusLg
                    fill: KodosiTheme.terminal
                    rim: false
                }
                Rectangle {
                    id: band

                    width: parent.width
                    height: 42
                    topLeftRadius: KodosiTheme.radiusLg
                    topRightRadius: KodosiTheme.radiusLg
                    color: KodosiTheme.terminalBand

                    Shape {
                        id: tab

                        readonly property Item current: tabs.count > 0
                            ? tabs.itemAt(Math.max(0, tabsRow.items.findIndex(entry => entry.id === root.agentId))) : null

                        visible: root.opened && current !== null
                        x: current ? tabsRow.x + current.x : 0
                        y: band.height - height
                        width: current ? current.width : 0
                        height: 34
                        preferredRendererType: Shape.CurveRenderer

                        Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                        Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                        ShapePath {
                            fillColor: KodosiTheme.terminal
                            strokeColor: "transparent"
                            startX: -8
                            startY: tab.height
                            PathQuad { x: 0; y: tab.height - 8; controlX: 0; controlY: tab.height }
                            PathLine { x: 0; y: 9 }
                            PathQuad { x: 9; y: 0; controlX: 0; controlY: 0 }
                            PathLine { x: tab.width - 9; y: 0 }
                            PathQuad { x: tab.width; y: 9; controlX: tab.width; controlY: 0 }
                            PathLine { x: tab.width; y: tab.height - 8 }
                            PathQuad { x: tab.width + 8; y: tab.height; controlX: tab.width; controlY: tab.height }
                        }
                    }
                    Row {
                        id: tabsRow

                        readonly property var items: [{ id: "", kind: "shell", installed: true }]
                            .concat(root.agents.filter(entry => entry.installed || root.showsAll || entry.agent === root.agentId)
                                .map(entry => ({ id: entry.agent, kind: entry.agent, installed: entry.installed })))
                        readonly property int hidden: root.agents.filter(entry => !entry.installed && entry.agent !== root.agentId).length

                        x: 10
                        y: band.height - height
                        height: 34
                        spacing: 2
                        visible: root.opened
                        objectName: "launcher.agents"
                        Accessible.role: Accessible.PageTabList
                        Accessible.name: qsTr("Agents")

                        Repeater {
                            id: tabs
                            model: tabsRow.items

                            AbstractButton {
                                id: mark

                                required property var modelData
                                readonly property bool selected: modelData.id === root.agentId
                                readonly property bool arrived: !selected && Models.Launcher.fresh.indexOf(modelData.id) >= 0
                                readonly property string title: modelData.kind === "shell" ? qsTr("Terminal") : Models.DesktopSettings.agentLabel(modelData.kind)
                                property real bounce: 1

                                objectName: "launcher.agent." + (modelData.id || "shell")
                                Accessible.id: objectName
                                Accessible.name: title
                                Accessible.role: Accessible.PageTab
                                Accessible.selected: selected
                                ToolTip.visible: hovered && !selected
                                ToolTip.text: modelData.installed ? title : qsTr("%1 is not on this computer").arg(title)
                                activeFocusOnTab: true
                                hoverEnabled: true
                                height: tabsRow.height
                                width: markRow.implicitWidth + (selected || arrived ? 24 : 16)
                                scale: pressed ? 0.96 : 1
                                clip: true

                                Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                                onSelectedChanged: {
                                    if (selected && !KodosiTheme.reduceMotion)
                                        squash.restart()
                                }
                                SequentialAnimation {
                                    id: squash
                                    NumberAnimation { target: mark; property: "bounce"; to: 0.8; duration: 90; easing.type: Easing.OutQuad }
                                    NumberAnimation { target: mark; property: "bounce"; to: 1; duration: KodosiTheme.motionSoft; easing.type: Easing.OutBack; easing.overshoot: 2.2 }
                                }

                                contentItem: Item {
                                    Row {
                                        id: markRow
                                        anchors.verticalCenter: parent.verticalCenter
                                        anchors.verticalCenterOffset: 1
                                        x: mark.selected || mark.arrived ? 12 : 8
                                        spacing: 7

                                        AgentMark {
                                            anchors.verticalCenter: parent.verticalCenter
                                            program: mark.modelData.kind === "shell" ? "" : mark.modelData.kind
                                            asleep: !mark.modelData.installed
                                            size: 18
                                            scale: mark.bounce
                                        }
                                        PlainLabel {
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: mark.selected || mark.arrived
                                            text: mark.title
                                            color: mark.arrived ? KodosiTheme.terminalAccent
                                                : mark.modelData.installed ? KodosiTheme.terminalInk : KodosiTheme.terminalInkMuted
                                            font.pixelSize: KodosiTheme.fontBody
                                            font.weight: Font.DemiBold
                                        }
                                    }
                                }
                                background: Item {
                                    Rectangle {
                                        x: 0
                                        y: 2
                                        width: parent.width
                                        height: parent.height - 4
                                        radius: KodosiTheme.radiusSm
                                        color: mark.hovered && !mark.selected ? "#2a221e" : "transparent"
                                        border.width: mark.visualFocus ? 1.5 : 0
                                        border.color: KodosiTheme.terminalAccent

                                        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                                    }
                                    Rectangle {
                                        id: ripple

                                        y: 2
                                        width: parent.width
                                        height: parent.height - 4
                                        radius: KodosiTheme.radiusSm
                                        color: "transparent"
                                        border.width: 1.5
                                        border.color: KodosiTheme.terminalAccent
                                        opacity: 0

                                        SequentialAnimation {
                                            running: mark.arrived && !KodosiTheme.reduceMotion
                                            loops: 2
                                            ParallelAnimation {
                                                NumberAnimation { target: ripple; property: "opacity"; from: 1; to: 0; duration: 1100; easing.type: Easing.OutCubic }
                                                NumberAnimation { target: ripple; property: "scale"; from: 1; to: 1.25; duration: 1100; easing.type: Easing.OutCubic }
                                            }
                                        }
                                    }
                                }
                                onClicked: {
                                    Models.Launcher.chooseAgent(modelData.id)
                                    promptInput.forceActiveFocus()
                                }
                            }
                        }
                        AbstractButton {
                            id: more

                            visible: tabsRow.hidden > 0
                            objectName: "launcher.more"
                            Accessible.id: objectName
                            Accessible.name: qsTr("More agents")
                            ToolTip.visible: hovered
                            ToolTip.text: root.showsAll ? qsTr("Hide agents that are not on this computer") : qsTr("Show %1 more agents").arg(tabsRow.hidden)
                            activeFocusOnTab: true
                            hoverEnabled: true
                            width: 30
                            height: tabsRow.height
                            onClicked: root.showsAll = !root.showsAll

                            contentItem: Item {
                                KIcon {
                                    anchors.centerIn: parent
                                    width: 13
                                    height: 13
                                    name: "plus"
                                    strokeWidth: 2.2
                                    color: more.hovered ? KodosiTheme.terminalInk : KodosiTheme.terminalInkMuted
                                    rotation: root.showsAll ? 45 : 0

                                    Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack } }
                                }
                            }
                            background: Rectangle {
                                y: 2
                                height: parent.height - 4
                                radius: KodosiTheme.radiusSm
                                color: more.hovered ? "#2a221e" : "transparent"
                                border.width: more.visualFocus ? 1.5 : 0
                                border.color: KodosiTheme.terminalAccent
                            }
                        }
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: 2
                        spacing: 5
                        visible: root.opened

                        KIcon { anchors.verticalCenter: parent.verticalCenter; width: 13; height: 13; name: "laptop"; strokeWidth: 1.9; color: KodosiTheme.terminalInkMuted }
                        PlainLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("This computer")
                            color: KodosiTheme.terminalInkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                        }
                    }
                }
                TapHandler {
                    id: bodyTap

                    onTapped: eventPoint => {
                        if (!deck.contains(deck.mapFromItem(bodyTap.parent, eventPoint.position)))
                            promptInput.forceActiveFocus()
                    }
                }
                Column {
                    id: terminalBody

                    x: 16
                    y: band.height + 14
                    width: preview.width - 32
                    spacing: 8
                    visible: root.opened

                    Item {
                        id: commandLine

                        property real rowEnd: 0
                        property real rowTop: 0
                        readonly property bool inline: width - rowEnd >= 150

                        width: parent.width
                        implicitHeight: !promptInput.visible ? keys.height
                            : inline ? Math.max(keys.height, rowTop + promptInput.height) : keys.height + 6 + promptInput.height

                        Flow {
                            id: keys

                            width: parent.width
                            spacing: 6
                            onPositioningComplete: {
                                let end = 0
                                let top = 0
                                for (const child of children) {
                                    if (!child.visible || child.width <= 0)
                                        continue
                                    if (child.y > top) {
                                        top = child.y
                                        end = 0
                                    }
                                    if (child.y === top)
                                        end = Math.max(end, child.x + child.width)
                                }
                                commandLine.rowEnd = end + spacing
                                commandLine.rowTop = top
                            }

                            TypedText {
                                height: 26
                                verticalAlignment: Text.AlignVCenter
                                target: root.folderTitle
                                color: "#5dafa6"
                                font: root.commandFont
                            }
                            Text {
                                height: 26
                                verticalAlignment: Text.AlignVCenter
                                text: "❯"
                                color: KodosiTheme.terminalAccent
                                font: root.commandFont
                                Accessible.ignored: true
                            }
                            Row {
                                visible: root.info !== null && root.info.installed
                                height: 26

                                TypedText {
                                    anchors.verticalCenter: parent.verticalCenter
                                    target: root.accountWord
                                    color: KodosiTheme.mix(Identity.kindTint(root.agentId), "#ffffff", 0.35)
                                    font: root.commandFont
                                }
                                TypedText {
                                    anchors.verticalCenter: parent.verticalCenter
                                    target: [root.modelArgument, root.effortArgument].filter(word => word.length > 0).map(word => " " + word).join("")
                                    color: KodosiTheme.terminalInkMuted
                                    font: root.commandFont
                                }
                            }
                            TypedText {
                                visible: root.info !== null && !root.info.installed
                                height: 26
                                verticalAlignment: Text.AlignVCenter
                                target: root.info ? root.info.install : ""
                                color: KodosiTheme.terminalInkMuted
                                font: root.commandFont
                            }
                            Item {
                                visible: !root.takesWords
                                width: 12
                                height: 26

                                CursorBlock {
                                    anchors.centerIn: parent
                                    width: 9
                                    height: 18
                                    blinks: true
                                    color: KodosiTheme.terminalAccent
                                }
                            }
                        }
                        LauncherPrompt {
                            id: promptInput

                            visible: root.takesWords
                            x: commandLine.inline ? commandLine.rowEnd : 0
                            y: commandLine.inline ? commandLine.rowTop : keys.height + 6
                            width: commandLine.inline ? commandLine.width - commandLine.rowEnd : commandLine.width
                            ghost: root.ghost
                            suggestion: root.suggestion
                            objectName: "launcher.prompt"
                            Accessible.id: objectName
                            Accessible.name: root.info ? qsTr("First prompt") : qsTr("Command")
                            onSubmitted: root.start()
                            onLengthChanged: {
                                if (length <= 1)
                                    root.typeHints()
                            }
                        }
                    }
                }
                Rectangle {
                    id: deck

                    y: band.height + preview.bodyHeight
                    width: parent.width
                    height: Math.max(54, deckKeys.implicitHeight + 20)
                    color: KodosiTheme.raised
                    bottomLeftRadius: KodosiTheme.radiusLg
                    bottomRightRadius: KodosiTheme.radiusLg
                    objectName: "launcher.deck"
                    Accessible.role: Accessible.Grouping
                    Accessible.id: objectName

                    Rectangle {
                        width: parent.width
                        height: 1
                        color: Qt.rgba(1, 1, 1, KodosiTheme.isDark ? 0.06 : 0.8)
                    }
                    Flow {
                        id: deckKeys

                        x: 12
                        y: 10
                        width: parent.width - 24
                        spacing: 8
                        opacity: root.opened ? 1 : 0
                        visible: opacity > 0

                        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSoft } }

                        PickerKey {
                            id: folderKey

                            title: root.folderTitle
                            open: root.menuKind === "folder"
                            objectName: "launcher.folder"
                            Accessible.id: objectName
                            Accessible.name: qsTr("Folder")
                            ToolTip.visible: hovered && !open
                            ToolTip.text: root.tilde(root.directory)
                            ToolTip.delay: 600
                            glyph: KIcon { width: 14; height: 14; name: "folder"; filled: true; strokeWidth: 1.2; color: "#4f9bb0" }
                            onClicked: root.toggleMenu("folder")
                        }
                        PickerKey {
                            id: accountKey

                            visible: root.info !== null && root.info.installed && (root.info.accounts.length > 1 || root.info.addsAccounts)
                            title: root.accountTitle(root.account)
                            open: root.menuKind === "account"
                            objectName: "launcher.account"
                            Accessible.id: objectName
                            Accessible.name: qsTr("Account")
                            ToolTip.visible: hovered && !open && !!root.account && !!root.account.identity
                            ToolTip.text: root.account ? root.account.identity || "" : ""
                            ToolTip.delay: 600
                            glyph: AccountMark {
                                size: 18
                                initial: accountKey.title.charAt(0)
                                key: root.account ? root.account.folder || root.account.command : ""
                                signedIn: !!root.account && !!root.account.identity
                            }
                            onClicked: root.toggleMenu("account")
                        }
                        PickerKey {
                            id: modelKey

                            visible: root.info !== null && root.info.installed && (root.listsModels || root.efforts.length > 0)
                            title: root.modelTitle.length > 0 ? root.modelTitle : qsTr("Model")
                            quiet: root.modelTitle.length === 0
                            loading: !root.modelSet && root.loadingModels
                            open: root.menuKind === "model"
                            objectName: "launcher.model"
                            Accessible.id: objectName
                            Accessible.name: qsTr("Model and effort")
                            glyph: KIcon { width: 12; height: 12; name: "sparkle"; filled: true; strokeWidth: 1; color: Identity.kindTint(root.agentId) }
                            accessory: EffortBars { visible: root.effortLevel >= 0; level: root.effortLevel; count: root.efforts.length }
                            onClicked: root.toggleMenu("model")
                        }
                    }
                }
            }
        }
        CursorBlock {
            x: preview.cursor.x
            y: preview.cursor.y
            width: preview.cursor.width
            height: preview.cursor.height
            color: KodosiTheme.terminalAccent
            opacity: preview.seed > 0 ? 0 : 1

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }
        }
    }

    RowLayout {
        Layout.alignment: root.welcome ? Qt.AlignHCenter : Qt.AlignLeft
        spacing: 10
        opacity: root.opened ? 1 : 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSoft } }

        KButton {
            objectName: "launcher.start"
            Accessible.id: objectName
            variant: KButton.Primary
            iconName: "arrow-right"
            iconTrailing: true
            enabled: root.ready
            text: !root.info ? qsTr("Open a terminal")
                : root.info.installed ? qsTr("Start %1").arg(Models.DesktopSettings.agentLabel(root.info.agent))
                : qsTr("Install %1").arg(Models.DesktopSettings.agentLabel(root.info.agent))
            onClicked: root.start()
        }
        KIconButton {
            glyph: "history"
            size: 32
            objectName: "launcher.resume"
            Accessible.id: objectName
            Accessible.name: qsTr("Resume a conversation")
            ToolTip.visible: hovered
            ToolTip.text: Accessible.name
            onClicked: root.resumeRequested()
        }
    }

    LauncherDropdown {
        id: folderPopover

        parent: folderKey
        onOpened: folderList.reset()
        onClosed: {
            if (root.menuKind === "folder")
                root.closeMenu()
        }
        contentItem: PickerList {
            id: folderList
            rows: root.folderRows()
            listWidth: 300
            objectName: "launcher.list.folder"
            Accessible.id: objectName
            onPicked: id => root.pick("folder", id)
            onClosed: root.closeMenu()
        }
    }
    LauncherDropdown {
        id: accountPopover

        parent: accountKey
        onOpened: accountList.reset()
        onClosed: {
            if (root.menuKind === "account")
                root.closeMenu()
        }
        contentItem: Item {
            implicitWidth: root.adding ? 340 : accountList.implicitWidth
            implicitHeight: root.adding ? newAccount.implicitHeight + 28 : accountList.implicitHeight

            PickerList {
                id: accountList
                visible: !root.adding
                width: parent.width
                rows: root.accountRows()
                listWidth: 330
                objectName: "launcher.list.account"
                Accessible.id: objectName
                onPicked: id => root.pick("account", id)
                onClosed: root.closeMenu()
            }
            ColumnLayout {
                id: newAccount

                readonly property string slug: accountName.text.trim().toLowerCase().split(/\s+/).filter(part => part.length > 0).join("-")
                readonly property string base: root.info && root.info.accounts.length > 0 && root.info.accounts[0].folder
                    ? root.info.accounts[0].folder : "~/." + (root.info ? root.info.program : "")

                function submit() {
                    if (slug.length > 0 && !Models.Launcher.addingAccount)
                        Models.Launcher.addAccount(root.info.agent, accountName.text)
                }

                visible: root.adding
                x: 14
                y: 14
                width: parent.width - 28
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10

                    AccountMark {
                        initial: newAccount.slug.length > 0 ? newAccount.slug.charAt(0) : "+"
                        key: newAccount.base + "-" + newAccount.slug
                        size: 30
                    }
                    KTextField {
                        id: accountName

                        Layout.fillWidth: true
                        placeholderText: qsTr("work, personal…")
                        objectName: "launcher.account.name"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Account name")
                        Keys.onEscapePressed: root.closeMenu()
                        onAccepted: newAccount.submit()
                    }
                    KButton {
                        variant: KButton.Primary
                        text: qsTr("Add")
                        working: Models.Launcher.addingAccount
                        enabled: newAccount.slug.length > 0 && !Models.Launcher.addingAccount
                        objectName: "launcher.account.add"
                        Accessible.id: objectName
                        onClicked: newAccount.submit()
                    }
                }
                PlainLabel {
                    Layout.fillWidth: true
                    Layout.leftMargin: 40
                    text: newAccount.slug.length === 0 ? qsTr("%1 asks you to sign in the first time.").arg(root.agentLabel)
                        : newAccount.base + "-" + newAccount.slug
                    color: KodosiTheme.inkFaint
                    font.family: newAccount.slug.length === 0 ? Application.font.family : "monospace"
                    font.pixelSize: KodosiTheme.fontFootnote
                    elide: Text.ElideMiddle
                }
                PlainLabel {
                    Layout.fillWidth: true
                    Layout.leftMargin: 40
                    visible: Models.Launcher.accountError.length > 0
                    text: Models.Launcher.accountError
                    color: KodosiTheme.caution
                    font.pixelSize: KodosiTheme.fontFootnote
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
    LauncherDropdown {
        id: modelPopover

        parent: modelKey
        onOpened: modelList.reset()
        onClosed: {
            if (root.menuKind === "model")
                root.closeMenu()
        }
        contentItem: Column {
            width: 290

            PickerList {
                id: modelList
                rows: root.modelRows()
                listWidth: 290
                objectName: "launcher.list.model"
                Accessible.id: objectName
                onPicked: id => root.pick("model", id)
                onClosed: root.closeMenu()
            }
            Rectangle {
                visible: root.efforts.length > 0
                width: parent.width
                height: 1
                color: KodosiTheme.alpha(KodosiTheme.hairline, 0.6)
            }
            EffortMeter {
                visible: root.efforts.length > 0
                width: parent.width
                levels: root.effortLevels()
                current: root.effortArgument
                objectName: "launcher.effort"
                Accessible.id: objectName
                onPicked: argument => root.pick("effort", argument)
            }
        }
    }
}
