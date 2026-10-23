pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    property bool centered: false
    property bool inSheet: false
    readonly property string folder: Models.DesktopState.launcherFolder
    property string roomId: ""
    property bool showsAll: false
    property string menuKind: ""
    property bool adding: false
    readonly property font commandFont: Qt.font({ family: "monospace", pixelSize: KodosiTheme.fontBody, weight: Font.Medium })
    readonly property string home: Models.DesktopSettings.homeDirectory
    readonly property var computers: Models.Devices.computers
    readonly property string computer: {
        const wanted = Models.DesktopSettings.lastComputer
        return computers.some(entry => entry.deviceId === wanted) ? wanted : ""
    }
    readonly property string computerName: (computers.find(entry => entry.deviceId === computer) || {}).label || qsTr("This computer")
    readonly property var launcher: computer.length > 0 ? Models.Launcher.remote : Models.Launcher
    readonly property var places: launcher.places
    readonly property var choice: root.launcher.choice
    readonly property var agents: root.launcher.agents
    readonly property string agentId: choice.agent || ""
    readonly property var info: agents.find(entry => entry.agent === agentId) || null
    readonly property var modelSet: info ? root.launcher.models[info.agent] : undefined
    readonly property bool loadingModels: info !== null && root.launcher.loadingModels.indexOf(info.agent) >= 0
    readonly property var plan: info ? root.launcher.plans[info.agent] || null : null
    readonly property var account: plan ? plan.account || null : null
    readonly property var accountUsage: info && account ? usageOf(account) : null
    readonly property string modelId: plan ? plan.modelId : ""
    readonly property var model: plan ? plan.model || null : null
    readonly property var efforts: plan ? plan.efforts : []
    readonly property string effortId: plan ? plan.effortId : ""
    readonly property string directory: computer.length > 0 ? Models.DesktopState.launcherRemoteFolders[computer] || (places.recent || [])[0] || ""
        : folder || Models.DesktopSettings.effectiveWorkingDirectory
    readonly property string agentLabel: info ? Models.DesktopSettings.agentLabel(info.agent) : ""
    readonly property string folderTitle: folderName(directory)
    readonly property string modelTitle: model ? model.label : modelId
    readonly property int effortLevel: efforts.findIndex(entry => entry.id === effortId)
    readonly property bool takesWords: !info || info.installed
    readonly property bool listsModels: !modelSet || modelSet.models.length > 0
    readonly property bool ready: !info ? agentId.length === 0 : !info.installed || !!(plan && plan.start)
    readonly property string hint: !info ? qsTr("Type a command") : qsTr("Type a task")
    readonly property string roomName: (Models.Missions.missions.find(room => room.id === roomId) || {}).name || ""

    signal closeRequested
    signal resumeRequested
    signal started

    function tilde(path) {
        return path === home ? "~" : path.startsWith(home + "/") ? "~" + path.slice(home.length) : path
    }
    function shown(path) {
        if (computer.length === 0)
            return tilde(path)
        const base = places.home || ""
        const separator = base.indexOf("\\") >= 0 ? "\\" : "/"
        const prefix = base.endsWith(separator) ? base : base + separator
        return !path || path === base ? "~" : base.length > 0 && path.startsWith(prefix) ? "~" + separator + path.slice(prefix.length) : path
    }
    function child(listing, name) {
        const separator = (listing.home || "").indexOf("\\") >= 0 ? "\\" : "/"
        return listing.path.endsWith(separator) ? listing.path + name : listing.path + separator + name
    }
    function folderName(path) {
        return shown(path) === "~" ? "~" : Models.Sessions.folderName(path)
    }
    function focusPrompt(path) {
        if (path)
            Models.DesktopState.launcherFolder = path
        promptInput.forceActiveFocus()
    }
    function showDraft() {
        const prompt = Models.DesktopState.launcherPrompt
        if (promptInput.text !== prompt) {
            promptInput.text = prompt
            promptInput.cursorPosition = promptInput.length
        }
    }
    function start() {
        if (!ready)
            return
        const words = promptInput.text.trim()
        const installed = !!info && info.installed
        const command = !info ? words : installed ? "" : info.install
        if (Models.SessionActions.create("", directory, command, installed ? words : "", roomId, installed ? plan.start : ({}), computer)) {
            Models.DesktopState.clearLauncherDraft()
            root.started()
        }
    }
    function accountTitle(entry) {
        return info && entry ? root.launcher.accountTitle(entry.name, info.program) : ""
    }
    function usageOf(entry) {
        const usage = entry.identity ? root.launcher.usage[info.agent + "/" + entry.name] : null
        return usage ? usage.headline : null
    }
    function accountRows() {
        if (!info)
            return []
        const rows = info.accounts.map(entry => {
            const title = accountTitle(entry)
            return { id: entry.name, kind: "choice", title: title,
                subtitle: entry.identity || qsTr("Signs in when it starts"),
                icon: "account", initial: title.charAt(0), key: entry.folder || entry.command, signedIn: !!entry.identity,
                usage: usageOf(entry), current: account !== null && entry.name === account.name }
        })
        if (info.addsAccounts)
            rows.push({ id: "menu.add", kind: "action", title: qsTr("Add an account"),
                subtitle: qsTr("Another subscription with its own sign-in"), icon: "slot", glyph: "plus" })
        return rows
    }
    function modelRows() {
        const rows = [{ id: "menu.default", kind: "choice", title: qsTr("Default"), current: modelId.length === 0 }]
        if (!modelSet)
            rows.push({ id: "menu.models", kind: "note", working: loadingModels,
                title: loadingModels ? qsTr("Asking %1 for its models").arg(agentLabel) : qsTr("%1 lists no models").arg(agentLabel) })
        for (const entry of modelSet ? modelSet.models : [])
            rows.push({ id: entry.id, kind: "choice", title: entry.label, current: entry.id === modelId,
                detail: entry.label.toLowerCase() === entry.id.toLowerCase() ? "" : entry.id })
        return rows
    }
    function effortLevels() {
        return efforts.map(entry => ({ id: entry.id,
            label: entry.id === "xhigh" ? qsTr("Extra high") : entry.id.charAt(0).toUpperCase() + entry.id.slice(1) }))
    }
    function folderRows() {
        if (computer.length > 0)
            return remoteRows()
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
    function remoteRows() {
        const listing = launcher.listing
        if (listing.path) {
            const rows = [{ id: "menu.use", kind: "action", title: qsTr("Use %1").arg(folderName(listing.path)), icon: "slot", glyph: "check" }]
            if (launcher.folderError.length > 0)
                rows.push({ id: "menu.failure", kind: "note", title: launcher.folderError })
            if (listing.parent)
                rows.push({ id: "menu.up", kind: "choice", title: folderName(listing.parent), icon: "slot", glyph: "chevron-up" })
            return rows.concat(listing.folders.map(name => ({ id: "folder:" + name, kind: "choice", title: name, icon: "folder" })))
        }
        const rows = (places.recent || []).map(path => ({ id: path, kind: "choice", icon: "folder", subtitle: shown(path),
            current: path === directory, title: folderName(path) }))
        if (!places.home)
            rows.push({ id: "menu.asking", kind: "note", working: launcher.folderError.length === 0,
                title: launcher.folderError.length > 0 ? launcher.folderError : qsTr("Asking %1 for its folders").arg(computerName) })
        return rows.concat([{ id: "menu.choose", kind: "action", title: qsTr("Choose a folder…"), icon: "slot", glyph: "folder" }])
    }
    function computerRows() {
        return [{ id: "menu.here", kind: "choice", icon: "device", title: qsTr("This computer"), current: computer.length === 0 }]
            .concat(computers.map(entry => ({ id: entry.deviceId, kind: "choice", icon: "device", title: entry.label,
                current: entry.deviceId === computer })))
    }
    function popover(kind) {
        return kind === "computer" ? computerPopover : kind === "folder" ? folderPopover : kind === "account" ? accountPopover : modelPopover
    }
    function toggleMenu(kind) {
        if (menuKind === kind) {
            closeMenu()
            return
        }
        adding = false
        menuKind = kind
        if (kind === "account")
            root.launcher.refreshUsage()
        popover(kind).open()
    }
    function closeMenu() {
        menuKind = ""
        adding = false
        for (const item of [computerPopover, folderPopover, accountPopover, modelPopover]) {
            if (item.visible)
                item.close()
        }
        if (computer.length > 0)
            launcher.closeFolders()
        promptInput.forceActiveFocus()
    }
    function pick(kind, id) {
        if (kind === "computer") {
            Models.DesktopSettings.lastComputer = id === "menu.here" ? "" : id
        } else if (kind === "folder" && computer.length > 0) {
            const listing = launcher.listing
            if (id === "menu.choose") {
                launcher.listFolders(places.home || "", true)
                return
            }
            if (id === "menu.up") {
                launcher.listFolders(listing.parent, true)
                return
            }
            if (id.startsWith("folder:")) {
                launcher.listFolders(child(listing, id.slice(7)), true)
                return
            }
            Models.DesktopState.setLauncherRemoteFolder(computer, id === "menu.use" ? listing.path : id)
        } else if (kind === "folder") {
            if (id === "menu.choose") {
                closeMenu()
                Models.DesktopFiles.requestDirectory("launcher", directory)
                return
            }
            Models.DesktopState.launcherFolder = id
        } else if (kind === "account") {
            if (id === "menu.add") {
                adding = true
                accountName.text = ""
                accountName.forceActiveFocus()
                return
            }
            root.launcher.choose("account", info.agent, id)
        } else if (kind === "model") {
            root.launcher.choose("model", info.agent, id === "menu.default" ? "" : id)
        } else if (kind === "effort") {
            root.launcher.choose("effort", info.agent, id)
        }
        closeMenu()
    }

    function askComputer() {
        if (computer.length === 0)
            return
        Models.Launcher.remote.computer = computer
        Models.Launcher.remote.listFolders()
    }

    spacing: centered ? 24 : 16
    objectName: "launcher"
    Accessible.id: objectName

    Component.onCompleted: {
        showDraft()
        askComputer()
        root.launcher.refresh(false)
        root.launcher.refreshUsage()
        root.launcher.loadModels(agentId)
    }
    onComputerChanged: {
        askComputer()
        if (menuKind.length > 0)
            closeMenu()
        root.launcher.refresh(false)
        root.launcher.refreshUsage()
    }
    onVisibleChanged: {
        if (visible) {
            askComputer()
            root.launcher.refresh(false)
            root.launcher.refreshUsage()
            promptInput.forceActiveFocus()
        }
    }
    onAgentIdChanged: {
        closeMenu()
        root.launcher.loadModels(agentId)
        promptInput.forceActiveFocus()
    }
    onInfoChanged: {
        root.launcher.loadModels(agentId)
        if (visible)
            root.launcher.refreshUsage()
    }

    Connections {
        target: root.launcher

        function onAccountAdded(agent, name) {
            root.closeMenu()
        }
        function onFoldersChanged() {
            if (folderPopover.visible)
                folderList.reset()
        }
    }
    Connections {
        target: Models.DesktopFiles

        function onDirectoryPicked(purpose, path) {
            if (purpose === "launcher" && root.visible)
                Models.DesktopState.launcherFolder = path
        }
    }
    Connections {
        target: Models.DesktopState

        function onLauncherDraftChanged() {
            root.showDraft()
        }
    }
    Connections {
        target: Qt.application

        function onStateChanged() {
            if (Qt.application.state === Qt.ApplicationActive)
                root.launcher.refresh(false)
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

        readonly property real bodyHeight: Math.max(root.centered ? 124 : 92, terminalBody.implicitHeight + 32)

        Layout.alignment: root.centered ? Qt.AlignHCenter : Qt.AlignLeft
        Layout.preferredWidth: Math.min(560, root.width)
        implicitHeight: band.height + bodyHeight
        objectName: "launcher.preview"
        Accessible.id: objectName

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

                visible: current !== null
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
                        readonly property string title: modelData.kind === "shell" ? qsTr("Terminal") : Models.DesktopSettings.agentLabel(modelData.kind)

                        objectName: "launcher.agent." + (modelData.id || "shell")
                        Accessible.id: objectName
                        Accessible.name: title
                        Accessible.role: Accessible.PageTab
                        Accessible.selected: selected
                        ToolTip.visible: hovered && !selected
                        ToolTip.delay: Tip.delay
                        ToolTip.text: modelData.installed ? title : qsTr("%1 is not on this computer").arg(title)
                        activeFocusOnTab: true
                        hoverEnabled: true
                        height: tabsRow.height
                        width: markRow.implicitWidth + (selected ? 24 : 16)
                        scale: pressed ? 0.96 : 1
                        clip: true

                        Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                        onSelectedChanged: {
                            if (selected && !KodosiTheme.reduceMotion)
                                squash.restart()
                        }
                        Squash {
                            id: squash
                            shape: markShape
                        }

                        contentItem: Item {
                            Row {
                                id: markRow
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.verticalCenterOffset: 1
                                x: mark.selected ? 12 : 8
                                spacing: 7

                                AgentMark {
                                    anchors.verticalCenter: parent.verticalCenter
                                    program: mark.modelData.kind === "shell" ? "" : mark.modelData.kind
                                    asleep: !mark.modelData.installed
                                    size: 18
                                    transform: Scale { id: markShape; origin.x: 9; origin.y: 9 }
                                }
                                PlainLabel {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: mark.selected
                                    text: mark.title
                                    color: mark.modelData.installed ? KodosiTheme.terminalInk : KodosiTheme.terminalInkMuted
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
                        }
                        onClicked: {
                            root.launcher.chooseAgent(modelData.id)
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
                    ToolTip.delay: Tip.delay
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

                            Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
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
                anchors.rightMargin: root.computers.length > 0 || root.inSheet ? 6 : 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 2
                spacing: 5
                objectName: "launcher.place"

                AbstractButton {
                    id: computerKey

                    readonly property bool open: root.menuKind === "computer"

                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.computers.length > 0
                    objectName: "launcher.computer"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Computer")
                    Accessible.description: root.computerName
                    ToolTip.visible: hovered && !open
                    ToolTip.delay: Tip.delay
                    ToolTip.text: qsTr("Computer")
                    activeFocusOnTab: true
                    hoverEnabled: true
                    height: 26
                    leftPadding: 8
                    rightPadding: 8
                    scale: pressed ? 0.96 : 1
                    onClicked: root.toggleMenu("computer")

                    contentItem: Row {
                        spacing: 5

                        KIcon { anchors.verticalCenter: parent.verticalCenter; width: 13; height: 13; name: "laptop"; strokeWidth: 1.9; color: KodosiTheme.terminalInkMuted }
                        PlainLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, 150)
                            text: root.computerName
                            elide: Text.ElideRight
                            color: computerKey.hovered || computerKey.open ? KodosiTheme.terminalInk : KodosiTheme.terminalInkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                        }
                        KIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 9
                            height: 9
                            name: "chevron-down"
                            strokeWidth: 2.4
                            color: computerKey.open ? KodosiTheme.terminalAccent : KodosiTheme.terminalInkMuted
                            rotation: computerKey.open ? 180 : 0

                            Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                        }
                    }
                    background: Rectangle {
                        radius: KodosiTheme.radiusSm
                        color: computerKey.hovered || computerKey.open ? "#2a221e" : "transparent"
                        border.width: computerKey.visualFocus ? 1.5 : 0
                        border.color: KodosiTheme.terminalAccent

                        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                    }
                }
                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.computers.length === 0
                    width: 13
                    height: 13
                    name: "laptop"
                    strokeWidth: 1.9
                    color: KodosiTheme.terminalInkMuted
                }
                PlainLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.computers.length === 0 || root.roomName.length > 0
                    text: root.computers.length > 0 ? "·" : root.roomName.length > 0 ? qsTr("This computer") + "  ·" : qsTr("This computer")
                    color: KodosiTheme.terminalInkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
                RoomSigil {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.roomName.length > 0
                    key: root.roomId
                    size: 13
                }
                PlainLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.roomName.length > 0
                    width: Math.min(implicitWidth, 160)
                    text: root.roomName
                    elide: Text.ElideRight
                    color: KodosiTheme.terminalInkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
                KIconButton {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.inSheet
                    glyph: "close"
                    onTerminal: true
                    size: 26
                    objectName: "launcher.close"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Close")

                    onClicked: root.closeRequested()
                }
            }
        }
        TapHandler {
            id: bodyTap

            onTapped: eventPoint => {
                if (!commandLine.contains(commandLine.mapFromItem(bodyTap.parent, eventPoint.position)))
                    promptInput.forceActiveFocus()
            }
        }
        Column {
            id: terminalBody

            x: 16
            y: band.height + 14
            width: preview.width - 32
            spacing: 8

            Item {
                id: commandLine

                property real rowEnd: 0
                property real rowTop: 0
                readonly property bool inline: root.info === null && width - rowEnd >= Math.max(120, promptInput.fitWidth)

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

                    PickerKey {
                        id: folderKey

                        title: root.folderTitle
                        open: root.menuKind === "folder"
                        objectName: "launcher.folder"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Folder")
                        ToolTip.visible: hovered && !open
                        ToolTip.text: root.shown(root.directory)
                        ToolTip.delay: Tip.delay
                        glyph: KIcon { width: 13; height: 13; name: "folder"; filled: true; strokeWidth: 1.2; color: "#4f9bb0" }
                        onClicked: root.toggleMenu("folder")
                    }
                    Text {
                        height: 26
                        verticalAlignment: Text.AlignVCenter
                        text: "❯"
                        color: KodosiTheme.terminalAccent
                        font: root.commandFont
                        Accessible.ignored: true
                    }
                    TypedText {
                        visible: root.info !== null && root.info.installed
                        height: 26
                        verticalAlignment: Text.AlignVCenter
                        target: root.info ? root.info.program : ""
                        color: KodosiTheme.mix(Identity.kindTint(root.agentId), "#ffffff", 0.35)
                        font: root.commandFont
                    }
                    PickerKey {
                        id: accountKey

                        visible: root.info !== null && root.info.installed && (root.info.accounts.length > 1 || root.info.addsAccounts)
                        title: root.accountTitle(root.account)
                        open: root.menuKind === "account"
                        objectName: "launcher.account"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Account")
                        ToolTip.visible: hovered && !open && ToolTip.text.length > 0
                        ToolTip.text: [root.account ? root.account.identity || "" : "", root.accountUsage ? root.accountUsage.spoken : ""]
                            .filter(part => part.length > 0).join("\n")
                        ToolTip.delay: Tip.delay
                        glyph: AccountMark {
                            size: root.accountUsage ? 20 : 16
                            Behavior on size { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                            initial: accountKey.title.charAt(0)
                            key: root.account ? root.account.folder || root.account.command : ""
                            signedIn: !!root.account && !!root.account.identity
                            usage: root.accountUsage
                            onTerminal: true
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
                        glyph: KIcon { width: 11; height: 11; name: "sparkle"; filled: true; strokeWidth: 1; color: Identity.kindTint(root.agentId) }
                        accessoryShown: root.effortLevel >= 0
                        accessory: EffortBars {
                            level: root.effortLevel
                            count: root.efforts.length
                            barHeight: 10
                            lit: KodosiTheme.terminalAccent
                            unlit: KodosiTheme.alpha(KodosiTheme.terminalInk, 0.2)
                        }
                        onClicked: root.toggleMenu("model")
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
                    hint: root.hint
                    clearsOnEscape: !root.inSheet
                    objectName: "launcher.prompt"
                    Accessible.id: objectName
                    Accessible.name: root.info ? qsTr("First prompt") : qsTr("Command")
                    onTextChanged: Models.DesktopState.launcherPrompt = text
                    onSubmitted: root.start()
                }
            }
        }
    }

    RowLayout {
        Layout.alignment: root.centered ? Qt.AlignHCenter : Qt.AlignLeft
        spacing: 10

        KButton {
            objectName: "launcher.start"
            Accessible.id: objectName
            variant: KButton.Primary
            iconName: "return"
            iconTrailing: true
            enabled: root.ready && !Models.SessionActions.busy
            working: Models.SessionActions.busy
            text: !root.info ? qsTr("Open a terminal")
                : root.info.installed ? qsTr("Start %1").arg(Models.DesktopSettings.agentLabel(root.info.agent))
                : qsTr("Install %1").arg(Models.DesktopSettings.agentLabel(root.info.agent))
            onClicked: root.start()
        }
        KIconButton {
            visible: root.computer.length === 0
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
        id: computerPopover

        parent: computerKey
        onOpened: computerList.reset()
        onClosed: {
            if (root.menuKind === "computer")
                root.closeMenu()
        }
        contentItem: PickerList {
            id: computerList
            rows: root.computerRows()
            listWidth: 250
            objectName: "launcher.list.computer"
            Accessible.id: objectName
            onPicked: id => root.pick("computer", id)
            onClosed: root.closeMenu()
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
            listWidth: root.computer.length > 0 ? 320 : 300
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
                    if (slug.length > 0 && !root.launcher.addingAccount)
                        root.launcher.addAccount(root.info.agent, accountName.text)
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
                        working: root.launcher.addingAccount
                        enabled: newAccount.slug.length > 0 && !root.launcher.addingAccount
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
                    visible: root.launcher.accountError.length > 0
                    text: root.launcher.accountError
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
                current: root.effortId
                objectName: "launcher.effort"
                Accessible.id: objectName
                onPicked: effort => root.pick("effort", effort)
            }
        }
    }
}
