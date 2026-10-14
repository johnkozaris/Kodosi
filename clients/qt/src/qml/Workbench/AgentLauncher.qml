pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtCore
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
    property bool showsEffort: false
    readonly property font commandFont: Qt.font({ family: "monospace", pixelSize: 14, weight: Font.Medium })
    readonly property string home: StandardPaths.writableLocation(StandardPaths.HomeLocation).toString().replace(/^file:\/\//, "")
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
    readonly property string identity: info && info.installed && account ? account.identity || "" : ""
    // Whether the person can type after the command: not while the line installs an agent.
    readonly property bool takesWords: !info || info.installed
    // Whether Start has a line to run: not while the chosen agent is still unknown.
    readonly property bool ready: !info ? agentId.length === 0 : line.length > 0
    readonly property var hints: !info ? ["git status", "npm run dev", "make test", "ls -la"]
        : [qsTr("fix the failing test"), qsTr("explain how this folder works"), qsTr("review my last commit"),
            qsTr("add a dark mode"), qsTr("write tests for the parser")]
    readonly property string suggestion: hints[hint % hints.length]
    // Whether the terminal stands open: after it grew from its cursor, or at once.
    property bool opened: false
    property string ghost: ""
    property int hint: 0

    signal resumeRequested

    function tilde(path) {
        return path === home ? "~" : path.startsWith(home + "/") ? "~" + path.slice(home.length) : path
    }
    function folderName(path) {
        const parts = path.split("/").filter(part => part.length > 0)
        return parts.length > 0 ? parts[parts.length - 1] : "/"
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
    // Types the suggestions into the empty line one after another, at a
    // person's uneven pace, and takes each away again.
    function typeHints() {
        typist.stop()
        ghost = ""
        if (!takesWords || promptInput.length > 0)
            return
        if (KodosiTheme.reduceMotion) {
            ghost = suggestion
            return
        }
        typist.phase = 0
        typist.interval = 1200
        typist.start()
    }
    function typeStep() {
        if (typist.phase === 0 && ghost.length < suggestion.length) {
            ghost = suggestion.slice(0, ghost.length + 1)
            typist.interval = ghost.endsWith(" ") ? 120 : 38 + Math.floor(Math.random() * 52)
        } else if (typist.phase === 0) {
            typist.phase = 1
            typist.interval = 2600
        } else if (ghost.length > 0) {
            ghost = ghost.slice(0, -1)
            typist.interval = 14
        } else {
            typist.phase = 0
            hint++
            typist.interval = 450
        }
        typist.start()
    }
    function pick(kind, value) {
        if (kind === "folder")
            folder = value
        else if (kind === "account")
            Models.Launcher.choose("accounts", info.agent, value)
        else if (kind === "model")
            Models.Launcher.choose("models", info.agent, value)
        else if (kind === "effort") {
            Models.Launcher.choose("efforts", info.agent, value)
            if (value.length === 0)
                showsEffort = false
        }
        menu.close()
    }
    function openMenu(kind, item) {
        let rows = []
        if (kind === "account") {
            rows = info.accounts.map(entry => ({ title: entry.name, value: entry.command, selected: entry.command === account.command,
                detail: entry.identity || "" }))
        } else if (kind === "model") {
            rows = [{ title: qsTr("Default"), detail: qsTr("The agent's own choice"), value: "", selected: modelArgument.length === 0 }]
                .concat((modelSet ? modelSet.models : []).map(entry => ({ title: entry.label, value: entry.argument, mono: true,
                    selected: entry.argument === modelArgument, detail: entry.label.toLowerCase() === entry.id.toLowerCase() ? "" : entry.id })))
        } else if (kind === "effort") {
            rows = [{ title: qsTr("Default"), detail: qsTr("The model's own level"), value: "", selected: effortArgument.length === 0 }]
                .concat(efforts.map(entry => ({ title: entry.id === "xhigh" ? qsTr("Extra high") : entry.id.charAt(0).toUpperCase() + entry.id.slice(1),
                    value: entry.argument, selected: entry.argument === effortArgument })))
        } else if (kind === "folder") {
            const seen = []
            const local = Models.Sessions.sessions.filter(session => session.kind === "local").map(session => session.workingDirectory || "")
            for (const path of [folder, Models.DesktopSettings.effectiveWorkingDirectory].concat(local).concat([home])) {
                if (path && seen.indexOf(path) < 0 && seen.length < 6)
                    seen.push(path)
            }
            rows = seen.map(path => ({ title: folderName(path), detail: tilde(path), mono: true, value: path, selected: path === directory }))
        }
        menu.kind = kind
        menu.rows = rows
        menu.filter = ""
        menu.adding = false
        menu.anchor = item
        const point = item.mapToItem(root, 0, item.height + 6)
        menu.x = Math.max(0, Math.min(point.x, root.width - menu.width))
        menu.y = point.y
        menu.open()
    }

    spacing: welcome ? 24 : 16
    objectName: "launcher"
    Accessible.id: objectName

    // The first time in a run of the app, the home's terminal grows from its
    // copper cursor, as in the film; after that it is simply there.
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
        showsEffort = false
        Models.Launcher.loadModels(agentId)
        typeHints()
        promptInput.forceActiveFocus()
    }
    onTakesWordsChanged: typeHints()
    onInfoChanged: Models.Launcher.loadModels(agentId)

    Timer {
        id: typist

        property int phase: 0

        onTriggered: root.typeStep()
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

    // The face of a key on the command line: one solid color, lit from the top
    // left as the buttons are.
    component Chip: Item {
        id: chip

        property bool lit: false
        property bool pressed: false
        property bool focused: false

        transform: Translate { y: chip.lit && !chip.pressed ? -0.5 : 0 }

        Bezel {
            anchors.fill: parent
            radius: KodosiTheme.radiusXs
            quiet: true
            dark: true
            pressed: chip.pressed
            fill: chip.lit ? "#352b26" : "#2a221e"
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            radius: KodosiTheme.radiusXs + 2
            color: "transparent"
            visible: chip.focused
            border.width: 1.5
            border.color: KodosiTheme.terminalAccent
        }
    }

    component Token: AbstractButton {
        id: token

        property string words: ""
        property string glyph: ""
        property string badge: ""
        property bool slot: false
        property bool loading: false
        property color tint: KodosiTheme.terminalInk
        property color glyphColor: KodosiTheme.terminalInkMuted
        property real maximumWidth: 480

        readonly property bool open: menu.opened && menu.anchor === token

        hoverEnabled: true
        activeFocusOnTab: true
        implicitHeight: 26
        implicitWidth: Math.min(maximumWidth, tokenRow.implicitWidth + 16)
        scale: pressed ? 0.95 : 1
        Accessible.role: Accessible.ButtonMenu
        Accessible.description: words + (badge.length > 0 ? " " + badge : "")

        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

        background: Chip {
            lit: token.hovered || token.open
            pressed: token.down
            focused: token.visualFocus
        }
        contentItem: Item {
            Row {
                id: tokenRow
                anchors.verticalCenter: parent.verticalCenter
                x: 8
                spacing: 5

                KIcon {
                    id: tokenIcon

                    anchors.verticalCenter: parent.verticalCenter
                    visible: token.glyph.length > 0
                    width: 12
                    height: 12
                    name: token.glyph
                    strokeWidth: 2.2
                    color: token.glyphColor

                    SequentialAnimation {
                        id: tokenBounce
                        NumberAnimation { target: tokenIcon; property: "scale"; to: 0.7; duration: 90; easing.type: Easing.OutQuad }
                        NumberAnimation { target: tokenIcon; property: "scale"; to: 1; duration: KodosiTheme.motionSoft; easing.type: Easing.OutBack; easing.overshoot: 2.4 }
                    }
                    Connections {
                        target: token

                        function onWordsChanged() {
                            if (tokenIcon.visible && !KodosiTheme.reduceMotion)
                                tokenBounce.restart()
                        }
                    }
                }
                ShimmerText {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: token.loading
                    text: token.words
                    active: true
                    color: KodosiTheme.terminalInkFaint
                    font: root.commandFont
                }
                TypedText {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !token.loading
                    width: Math.min(implicitWidth, token.maximumWidth - (token.glyph.length > 0 ? 58 : 40))
                    target: token.words
                    color: token.slot ? KodosiTheme.terminalInkMuted : token.tint
                    font: root.commandFont
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: token.badge.length > 0
                    text: "· " + token.badge
                    textFormat: Text.PlainText
                    color: KodosiTheme.terminalInkMuted
                    font: root.commandFont
                }
                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8
                    height: 8
                    name: "chevron-down"
                    strokeWidth: 2.8
                    color: KodosiTheme.terminalInkMuted
                    rotation: token.open ? 180 : 0

                    Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                }
            }
        }
    }

    // The terminal about to open, as its tile will look: a band with a tab for
    // each kind of terminal, then the prompt with the folder, the command and
    // the cursor.
    Item {
        id: preview

        // From 0, the copper cursor alone, through a line, to 1, the whole window.
        property real seed: 0
        readonly property rect cursor: Qt.rect(16, band.height + 18, 9, 18)
        readonly property real wide: Math.min(1, Math.max(0, seed / 0.45))
        readonly property real tall: Math.min(1, Math.max(0, (seed - 0.45) / 0.55))

        Layout.alignment: root.welcome ? Qt.AlignHCenter : Qt.AlignLeft
        Layout.preferredWidth: Math.min(560, root.width)
        implicitHeight: band.height + Math.max(root.welcome ? 150 : 104, terminalBody.implicitHeight + 32)
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

            // The window shows through a frame that grows from the cursor; at the
            // end it reaches past the window, so its shadow shows.
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
                    radius: KodosiTheme.radiusTile
                    fill: KodosiTheme.terminal
                    rim: false
                }
                Rectangle {
                    id: band

                    width: parent.width
                    height: 42
                    topLeftRadius: KodosiTheme.radiusTile
                    topRightRadius: KodosiTheme.radiusTile
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
                                readonly property string title: modelData.kind === "shell" ? qsTr("Terminal") : Identity.kindLabel(modelData.kind)
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
                                            font.pixelSize: KodosiTheme.fontFootnote
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
                                    // A ring that leaves its place, for an agent that just arrived.
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
                            font.pixelSize: KodosiTheme.fontCaption
                        }
                    }
                }
                TapHandler {
                    onTapped: promptInput.forceActiveFocus()
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

                        // Where the last row of words ends; the prompt takes the rest of
                        // that row when there is room, else a row of its own.
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

                            Token {
                                id: folderToken
                                maximumWidth: 240
                                glyph: "folder"
                                glyphColor: "#5dafa6"
                                words: root.tilde(root.directory)
                                objectName: "launcher.folder"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Folder")
                                onClicked: root.openMenu("folder", folderToken)
                            }
                            Text {
                                height: 26
                                verticalAlignment: Text.AlignVCenter
                                text: "❯"
                                color: KodosiTheme.terminalAccent
                                font: root.commandFont
                                Accessible.ignored: true
                            }
                            Token {
                                id: accountToken
                                visible: root.info !== null && root.info.installed && root.account !== null
                                words: !root.account ? "" : root.account.command === root.account.name ? root.account.name : root.info.program
                                badge: !root.account || root.account.command === root.account.name ? "" : root.account.name
                                tint: root.info ? KodosiTheme.mix(Identity.kindTint(root.info.agent), "#ffffff", 0.35) : KodosiTheme.terminalInk
                                objectName: "launcher.account"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Account")
                                onClicked: root.openMenu("account", accountToken)
                            }
                            Token {
                                id: modelToken
                                visible: root.info !== null && root.info.installed && (!root.modelSet || root.modelSet.models.length > 0)
                                words: root.modelArgument || qsTr("model")
                                slot: root.modelArgument.length === 0
                                loading: root.loadingModels
                                objectName: "launcher.model"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Model")
                                onClicked: root.openMenu("model", modelToken)
                            }
                            Token {
                                id: effortToken
                                visible: root.info !== null && root.info.installed && root.efforts.length > 0
                                    && (root.effortArgument.length > 0 || root.showsEffort)
                                words: root.effortArgument || qsTr("effort")
                                slot: root.effortArgument.length === 0
                                objectName: "launcher.effort"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Effort")
                                onClicked: root.openMenu("effort", effortToken)
                            }
                            AbstractButton {
                                id: moreChoices

                                visible: root.info !== null && root.info.installed && root.efforts.length > 0
                                    && root.effortArgument.length === 0 && !root.showsEffort
                                implicitWidth: 30
                                implicitHeight: 26
                                hoverEnabled: true
                                activeFocusOnTab: true
                                scale: pressed ? 0.95 : 1
                                objectName: "launcher.moreChoices"
                                Accessible.id: objectName
                                Accessible.name: qsTr("More choices")
                                ToolTip.visible: hovered
                                ToolTip.text: Accessible.name
                                onClicked: root.showsEffort = true

                                Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

                                background: Chip { lit: moreChoices.hovered; pressed: moreChoices.down; focused: moreChoices.visualFocus }
                                contentItem: Item {
                                    KIcon { anchors.centerIn: parent; width: 13; height: 13; name: "more"; strokeWidth: 2.4; color: KodosiTheme.terminalInkMuted }
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
                    PlainLabel {
                        width: parent.width
                        visible: root.identity.length > 0
                        text: "# " + root.identity
                        color: KodosiTheme.terminalInkFaint
                        font.family: "monospace"
                        font.pixelSize: KodosiTheme.fontFootnote
                        elide: Text.ElideMiddle
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

            Behavior on opacity { NumberAnimation { duration: 150 } }
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
            large: true
            iconName: "arrow-right"
            iconTrailing: true
            enabled: root.ready
            text: !root.info ? qsTr("Open a terminal")
                : root.info.installed ? qsTr("Start %1").arg(Identity.kindLabel(root.info.agent))
                : qsTr("Install %1").arg(Identity.kindLabel(root.info.agent))
            onClicked: root.start()
        }
        KIconButton {
            glyph: "history"
            size: 36
            objectName: "launcher.resume"
            Accessible.id: objectName
            Accessible.name: qsTr("Resume a conversation")
            ToolTip.visible: hovered
            ToolTip.text: Accessible.name
            onClicked: root.resumeRequested()
        }
    }

    KPopover {
        id: menu

        property string kind: ""
        property var rows: []
        property string filter: ""
        property bool adding: false
        property Item anchor: null
        readonly property var shown: filter.trim().length === 0 ? rows
            : rows.filter(row => (row.title + " " + (row.detail || "")).toLowerCase().indexOf(filter.trim().toLowerCase()) >= 0)

        objectName: "launcher.menu"
        width: 300
        padding: 8
        margins: 12
        focus: true
        onClosed: promptInput.forceActiveFocus()

        Connections {
            target: Models.Launcher

            function onAccountAdded(agent, command) {
                menu.close()
            }
        }

        contentItem: ColumnLayout {
            spacing: 4

            PlainLabel {
                Layout.leftMargin: 10
                text: menu.kind === "account" ? qsTr("Account") : menu.kind === "model" ? qsTr("Model")
                    : menu.kind === "effort" ? qsTr("Effort") : qsTr("Folder")
                color: KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
                font.weight: Font.Medium
            }
            KTextField {
                Layout.fillWidth: true
                visible: menu.kind === "model" && menu.rows.length > 9
                placeholderText: qsTr("Find a model")
                objectName: "launcher.model.filter"
                Accessible.id: objectName
                text: menu.filter
                onTextEdited: menu.filter = text
            }
            ShimmerText {
                Layout.margins: 10
                visible: menu.kind === "model" && root.loadingModels
                text: qsTr("Asking the agent")
                active: true
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
            }
            ListView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, 300)
                clip: true
                model: menu.shown
                boundsBehavior: Flickable.StopAtBounds

                delegate: AbstractButton {
                    id: row

                    required property var modelData
                    required property int index

                    width: ListView.view.width
                    height: rowColumn.implicitHeight + 12
                    hoverEnabled: true
                    objectName: "launcher." + menu.kind + "." + index
                    Accessible.id: objectName
                    Accessible.name: modelData.title
                    Accessible.selected: modelData.selected === true
                    onClicked: root.pick(menu.kind, modelData.value)

                    background: Rectangle {
                        radius: KodosiTheme.radiusSm
                        color: KodosiTheme.alpha(KodosiTheme.ink, row.hovered ? 0.06 : 0)
                    }
                    contentItem: RowLayout {
                        spacing: 10

                        KIcon {
                            Layout.leftMargin: 8
                            Layout.preferredWidth: 12
                            Layout.preferredHeight: 12
                            name: "check"
                            strokeWidth: 2.4
                            color: KodosiTheme.accentStrong
                            opacity: row.modelData.selected === true ? 1 : 0
                        }
                        ColumnLayout {
                            id: rowColumn
                            Layout.fillWidth: true
                            spacing: 1

                            PlainLabel { id: rowTitle; Layout.fillWidth: true; text: row.modelData.title; elide: Text.ElideRight }
                            PlainLabel {
                                Layout.fillWidth: true
                                visible: !!row.modelData.detail
                                text: row.modelData.detail || ""
                                color: KodosiTheme.inkMuted
                                font.family: row.modelData.mono === true ? "monospace" : rowTitle.font.family
                                font.pixelSize: KodosiTheme.fontCaption
                                elide: Text.ElideMiddle
                            }
                        }
                    }
                }
            }
            AbstractButton {
                id: extraRow

                Layout.fillWidth: true
                visible: menu.kind === "folder" || (menu.kind === "account" && root.info !== null && root.info.addsAccounts && !menu.adding)
                implicitHeight: 32
                hoverEnabled: true
                objectName: menu.kind === "folder" ? "launcher.folder.choose" : "launcher.account.new"
                Accessible.id: objectName
                Accessible.name: menu.kind === "folder" ? qsTr("Choose a folder…") : qsTr("Add an account")
                onClicked: {
                    if (menu.kind === "folder") {
                        menu.close()
                        Models.DesktopFiles.requestDirectory("launcher", root.directory)
                    } else {
                        menu.adding = true
                        accountName.text = ""
                        accountName.forceActiveFocus()
                    }
                }

                background: Rectangle {
                    radius: KodosiTheme.radiusSm
                    color: KodosiTheme.alpha(KodosiTheme.ink, extraRow.hovered ? 0.06 : 0)
                }
                contentItem: RowLayout {
                    spacing: 10
                    KIcon {
                        Layout.leftMargin: 8
                        Layout.preferredWidth: 12
                        Layout.preferredHeight: 12
                        name: menu.kind === "folder" ? "folder" : "plus"
                        strokeWidth: 2.2
                        color: KodosiTheme.accentStrong
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        text: menu.kind === "folder" ? qsTr("Choose a folder…") : qsTr("Add an account")
                        color: KodosiTheme.accentStrong
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: 6
                visible: menu.kind === "account" && menu.adding
                spacing: 8

                RowLayout {
                    spacing: 8
                    KTextField {
                        id: accountName
                        Layout.fillWidth: true
                        placeholderText: qsTr("Name, such as work")
                        objectName: "launcher.account.name"
                        Accessible.id: objectName
                        onAccepted: Models.Launcher.addAccount(root.info.agent, text)
                    }
                    KButton {
                        objectName: "launcher.account.add"
                        Accessible.id: objectName
                        variant: KButton.Primary
                        compact: true
                        text: qsTr("Add")
                        enabled: accountName.text.trim().length > 0 && !Models.Launcher.addingAccount
                        onClicked: Models.Launcher.addAccount(root.info.agent, accountName.text)
                    }
                }
                PlainLabel {
                    Layout.fillWidth: true
                    visible: Models.Launcher.accountError.length > 0
                    text: Models.Launcher.accountError
                    color: KodosiTheme.danger
                    font.pixelSize: KodosiTheme.fontCaption
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
}
