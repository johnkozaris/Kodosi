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
    property bool showsMore: false
    property string menuKind: ""
    property real menuX: 0
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
    readonly property string identity: info && info.installed && account ? account.identity || "" : ""
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
    function accountLines() {
        if (!info)
            return []
        const groups = []
        for (const entry of info.accounts) {
            const key = entry.folder || ""
            let group = groups.find(item => item.folder === key)
            if (!group) {
                group = { folder: key, accounts: [] }
                groups.push(group)
            }
            group.accounts.push(entry)
        }
        const folders = info.accounts.some(entry => !!entry.folder)
        const lines = []
        for (const group of groups) {
            if (folders) {
                const signed = group.accounts.find(entry => !!entry.identity)
                lines.push({ id: "sign-in." + group.folder, kind: "comment", detail: group.folder,
                    title: group.folder.length === 0 ? qsTr("a sign-in that your shell chooses")
                        : signed ? signed.identity : qsTr("not signed in yet") })
            }
            for (const entry of group.accounts)
                lines.push({ id: entry.command, kind: "choice", title: entry.name, detail: entry.options || "",
                    current: account !== null && entry.command === account.command })
        }
        if (info.addsAccounts)
            lines.push({ id: "menu.add", kind: "action", title: qsTr("+ new account") })
        return lines
    }
    function modelLines() {
        const lines = [{ id: "menu.default", kind: "choice", title: qsTr("default"), detail: qsTr("the agent's own choice"),
            current: modelArgument.length === 0 }]
        if (!modelSet)
            lines.push({ id: "menu.models", kind: "comment",
                title: loadingModels ? qsTr("asking the agent for its models") : qsTr("the agent did not list its models") })
        for (const entry of modelSet ? modelSet.models : [])
            lines.push({ id: entry.argument, kind: "choice", title: entry.label, current: entry.argument === modelArgument,
                detail: entry.label.toLowerCase() === entry.id.toLowerCase() ? "" : entry.id })
        return lines
    }
    function effortLines() {
        return [{ id: "menu.default", kind: "choice", title: qsTr("default"), detail: qsTr("the model's own level"),
            current: effortArgument.length === 0 }]
            .concat(efforts.map(entry => ({ id: entry.argument, kind: "choice", current: entry.argument === effortArgument,
                title: entry.id === "xhigh" ? qsTr("extra high") : entry.id })))
    }
    function folderLines() {
        const seen = []
        const local = Models.Sessions.sessions.filter(session => session.kind === "local").map(session => session.workingDirectory || "")
        for (const path of [folder, Models.DesktopSettings.effectiveWorkingDirectory].concat(local).concat([home])) {
            if (path && seen.indexOf(path) < 0 && seen.length < 6)
                seen.push(path)
        }
        return seen.map(path => ({ id: path, kind: "choice", title: tilde(path), current: path === directory }))
            .concat([{ id: "menu.choose", kind: "action", title: qsTr("… another folder") }])
    }
    function toggleMenu(kind, key) {
        adding = false
        if (menuKind === kind) {
            closeMenu()
            return
        }
        menuX = key.mapToItem(terminalBody, 0, 0).x
        menuKind = kind
    }
    function closeMenu() {
        menuKind = ""
        adding = false
        promptInput.forceActiveFocus()
    }
    function pick(id) {
        if (menuKind === "folder") {
            if (id === "menu.choose") {
                closeMenu()
                Models.DesktopFiles.requestDirectory("launcher", directory)
                return
            }
            folder = id
        } else if (menuKind === "account") {
            if (id === "menu.add") {
                adding = true
                accountName.text = ""
                accountName.forceActiveFocus()
                return
            }
            Models.Launcher.choose("accounts", info.agent, id)
        } else if (menuKind === "model") {
            Models.Launcher.choose("models", info.agent, id === "menu.default" ? "" : id)
        } else if (menuKind === "effort") {
            Models.Launcher.choose("efforts", info.agent, id === "menu.default" ? "" : id)
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
        showsMore = false
        menuKind = ""
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
        property string kind: ""

        readonly property bool open: kind.length > 0 && root.menuKind === kind

        hoverEnabled: true
        activeFocusOnTab: true
        focusPolicy: Qt.TabFocus
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

    Item {
        id: preview

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
                        const inside = item => {
                            const point = item.mapFromItem(bodyTap.parent, eventPoint.position)
                            return item.visible && item.contains(point)
                        }
                        if (inside(commandLine) || inside(menuLoader) || inside(newAccount))
                            return
                        if (root.menuKind.length > 0)
                            root.closeMenu()
                        else
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

                            Token {
                                id: folderToken
                                maximumWidth: 240
                                glyph: "folder"
                                glyphColor: "#5dafa6"
                                words: root.tilde(root.directory) === "~" ? "~" : Models.Sessions.folderName(root.directory)
                                kind: "folder"
                                objectName: "launcher.folder"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Folder")
                                onClicked: root.toggleMenu("folder", folderToken)
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
                                kind: "account"
                                objectName: "launcher.account"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Account")
                                onClicked: root.toggleMenu("account", accountToken)
                            }
                            Token {
                                id: modelToken
                                visible: root.info !== null && root.info.installed
                                    && (root.modelArgument.length > 0 || (root.showsMore && root.listsModels))
                                words: root.modelArgument || qsTr("model")
                                slot: root.modelArgument.length === 0
                                loading: root.loadingModels
                                kind: "model"
                                objectName: "launcher.model"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Model")
                                onClicked: root.toggleMenu("model", modelToken)
                            }
                            Token {
                                id: effortToken
                                visible: root.info !== null && root.info.installed
                                    && (root.effortArgument.length > 0 || (root.showsMore && root.efforts.length > 0))
                                words: root.effortArgument || qsTr("effort")
                                slot: root.effortArgument.length === 0
                                kind: "effort"
                                objectName: "launcher.effort"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Effort")
                                onClicked: root.toggleMenu("effort", effortToken)
                            }
                            AbstractButton {
                                id: moreChoices

                                visible: root.info !== null && root.info.installed && !root.showsMore
                                    && ((root.modelArgument.length === 0 && root.listsModels)
                                        || (root.effortArgument.length === 0 && root.efforts.length > 0))
                                implicitWidth: 30
                                implicitHeight: 26
                                hoverEnabled: true
                                activeFocusOnTab: true
                                scale: pressed ? 0.95 : 1
                                objectName: "launcher.moreChoices"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Model and effort")
                                ToolTip.visible: hovered
                                ToolTip.text: Accessible.name
                                onClicked: root.showsMore = true

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
                            onActiveFocusChanged: {
                                if (activeFocus && root.menuKind.length > 0)
                                    root.closeMenu()
                            }
                        }
                    }
                    Loader {
                        id: menuLoader

                        x: Math.max(0, Math.min(root.menuX, terminalBody.width - 400))
                        active: root.menuKind.length > 0 && !root.adding
                        visible: active

                        sourceComponent: TerminalMenu {
                            room: terminalBody.width - menuLoader.x
                            lines: root.menuKind === "account" ? root.accountLines() : root.menuKind === "model" ? root.modelLines()
                                : root.menuKind === "effort" ? root.effortLines() : root.folderLines()
                            objectName: "launcher.list." + root.menuKind
                            Accessible.id: objectName
                            onPicked: id => root.pick(id)
                            onClosed: root.closeMenu()
                        }
                    }
                    Column {
                        id: newAccount

                        readonly property string slug: accountName.text.trim().toLowerCase().split(/\s+/).filter(part => part.length > 0).join("-")

                        x: menuLoader.x
                        visible: root.menuKind === "account" && root.adding

                        Row {
                            height: 24

                            Text {
                                width: 20
                                anchors.verticalCenter: parent.verticalCenter
                                horizontalAlignment: Text.AlignRight
                                text: "+ "
                                color: KodosiTheme.terminalAccent
                                font: root.commandFont
                            }
                            TextInput {
                                id: accountName

                                width: 200
                                anchors.verticalCenter: parent.verticalCenter
                                color: KodosiTheme.terminalInk
                                selectionColor: KodosiTheme.alpha(KodosiTheme.terminalAccent, 0.35)
                                font: root.commandFont
                                clip: true
                                objectName: "launcher.account.name"
                                Accessible.id: objectName
                                Accessible.name: qsTr("Account name")
                                Keys.onEscapePressed: {
                                    root.adding = false
                                    Qt.callLater(() => { if (menuLoader.item) menuLoader.item.forceActiveFocus() })
                                }
                                onAccepted: {
                                    if (newAccount.slug.length > 0 && !Models.Launcher.addingAccount)
                                        Models.Launcher.addAccount(root.info.agent, text)
                                }

                                Text {
                                    visible: accountName.length === 0
                                    text: qsTr("name")
                                    color: KodosiTheme.terminalInkFaint
                                    font: root.commandFont
                                }
                            }
                        }
                        Text {
                            x: 20
                            height: 24
                            verticalAlignment: Text.AlignVCenter
                            text: "# " + (newAccount.slug.length === 0 ? qsTr("a sign-in of its own")
                                : (root.info && root.info.accounts.length > 0 && root.info.accounts[0].folder
                                    ? root.info.accounts[0].folder : "~/." + (root.info ? root.info.program : "")) + "-" + newAccount.slug)
                            textFormat: Text.PlainText
                            color: KodosiTheme.terminalInkFaint
                            font.family: "monospace"
                            font.pixelSize: KodosiTheme.fontBody
                        }
                        Text {
                            x: 20
                            width: terminalBody.width - newAccount.x - 20
                            visible: Models.Launcher.accountError.length > 0
                            text: "# " + Models.Launcher.accountError
                            textFormat: Text.PlainText
                            color: KodosiTheme.terminalCaution
                            font.family: "monospace"
                            font.pixelSize: KodosiTheme.fontBody
                            wrapMode: Text.WordWrap
                        }
                    }
                    PlainLabel {
                        width: parent.width
                        visible: root.identity.length > 0 && root.menuKind.length === 0
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
}
