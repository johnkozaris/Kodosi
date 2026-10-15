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
    property string folder: ""
    property string roomId: ""
    property bool showsAll: false
    property bool asked: false
    property string error: ""
    readonly property font commandFont: Qt.font({ family: "monospace", pixelSize: KodosiTheme.fontBody, weight: Font.Medium })
    readonly property string home: Models.DesktopSettings.homeDirectory
    readonly property string agentId: Models.Launcher.choice.agent || ""
    readonly property var agents: Models.Launcher.agents
    readonly property var info: agents.find(entry => entry.agent === agentId) || null
    readonly property var plan: info ? Models.Launcher.plans[info.agent] || null : null
    readonly property var offer: info ? Models.Launcher.models[info.agent] || null : null
    readonly property string label: info ? Models.DesktopSettings.agentLabel(info.agent) : ""
    readonly property string directory: folder || Models.DesktopSettings.effectiveWorkingDirectory
    readonly property bool installed: info !== null && info.installed && plan !== null
    readonly property bool takesWords: !info || info.installed
    readonly property bool ready: !info ? agentId.length === 0 : plan !== null && plan.line.length > 0
    readonly property bool hasOptions: installed && (info.addsAccounts || !offer || offer.models.length > 0 || plan.efforts.length > 0)
    readonly property string roomName: (Models.Missions.missions.find(room => room.id === roomId) || {}).name || ""

    signal resumeRequested
    signal started

    function tilde(path) {
        return path === home ? "~" : path.startsWith(home + "/") ? "~" + path.slice(home.length) : path
    }
    function focusPrompt(path) {
        if (path)
            folder = path
        promptInput.forceActiveFocus()
    }
    function start() {
        if (!ready)
            return
        const words = promptInput.text.trim()
        let command = info ? plan.line : words
        let first = ""
        if (info && info.installed && words.length > 0) {
            command = [plan.line, info.promptFlag || ""].filter(word => word.length > 0).join(" ")
            first = words
        }
        error = ""
        if (Models.SessionActions.create("", directory, command, first, roomId)) {
            promptInput.clear()
            root.started()
        }
    }
    function accountTitle(account) {
        return account.identity || account.folder || account.name
    }
    function accountDetail(account) {
        return account.identity ? account.folder || "" : qsTr("Not signed in")
    }
    function effortLabel(effort) {
        return effort.id === "xhigh" ? qsTr("Extra high") : effort.id.charAt(0).toUpperCase() + effort.id.slice(1)
    }
    function recentFolders() {
        const seen = []
        const local = Models.Sessions.sessions.filter(session => session.kind === "local").map(session => session.workingDirectory || "")
        for (const path of [folder, Models.DesktopSettings.effectiveWorkingDirectory].concat(local).concat([home])) {
            if (path && seen.indexOf(path) < 0 && seen.length < 6)
                seen.push(path)
        }
        return seen
    }
    function modelChoices() {
        return [{ kind: "model", value: "", text: qsTr("Default"), checked: plan.modelArgument.length === 0 }]
            .concat((offer ? offer.models : []).map(model => ({ kind: "model", value: model.argument, text: model.label,
                checked: model.argument === plan.modelArgument })))
    }
    function effortChoices() {
        return [{ kind: "effort", value: "", text: qsTr("Default"), checked: plan.effortArgument.length === 0 }]
            .concat(plan.efforts.map(effort => ({ kind: "effort", value: effort.argument, text: root.effortLabel(effort),
                checked: effort.argument === plan.effortArgument })))
    }
    function menuEntries(kind) {
        const adding = info && info.addsAccounts ? [{ kind: "separator" }, { kind: "addAccount", text: qsTr("Add Account…") }] : []
        switch (kind) {
        case "folder":
            return recentFolders().map(path => ({ kind: "folder", value: path, text: tilde(path), checked: path === directory }))
                .concat([{ kind: "separator" }, { kind: "chooseFolder", text: qsTr("Choose Another Folder…") }])
        case "account":
            return info.accounts.map(account => ({ kind: "account", value: account.command, text: accountTitle(account),
                detail: accountDetail(account), checked: !!plan.account && account.command === plan.account.command }))
                .concat(adding)
        case "model":
            return modelChoices()
        case "effort":
            return effortChoices()
        }
        const choices = !offer
            ? [{ kind: "note", enabled: false, text: Models.Launcher.loadingModels.indexOf(info.agent) >= 0
                ? qsTr("Asking %1 for its models…").arg(label) : qsTr("%1 did not list its models").arg(label) }]
            : (offer.models.length > 0 ? [{ text: qsTr("Model"), entries: modelChoices() }] : [])
                .concat(plan.efforts.length > 0 ? [{ text: qsTr("Effort"), entries: effortChoices() }] : [])
        return choices.length > 0 ? choices.concat(adding) : adding.slice(1)
    }
    function fillMenu(target, entries) {
        for (const entry of entries) {
            if (entry.kind === "separator") {
                target.addItem(menuSeparator.createObject(target))
            } else if (entry.entries) {
                const submenu = menuPart.createObject(target, { title: entry.text })
                fillMenu(submenu, entry.entries)
                target.addMenu(submenu)
            } else {
                target.addItem(menuChoice.createObject(target, { text: entry.text, detail: entry.detail || "",
                    checked: entry.checked === true, enabled: entry.enabled !== false, entry: entry }))
            }
        }
    }
    function emptyMenu(target) {
        while (target.count > 0) {
            const item = target.takeItem(0)
            if (item.subMenu) {
                emptyMenu(item.subMenu)
                item.subMenu.destroy()
            }
            item.destroy()
        }
    }
    function showMenu(kind, key) {
        emptyMenu(menu)
        menu.kind = kind
        fillMenu(menu, menuEntries(kind))
        menu.popup(key, 0, key.height + 4)
    }
    function pick(entry) {
        switch (entry.kind) {
        case "folder":
            folder = entry.value
            break
        case "chooseFolder":
            Models.DesktopFiles.requestDirectory("launcher", directory)
            break
        case "account":
            Models.Launcher.choose("accounts", info.agent, entry.value)
            break
        case "model":
            Models.Launcher.choose("models", info.agent, entry.value)
            break
        case "effort":
            Models.Launcher.choose("efforts", info.agent, entry.value)
            break
        case "addAccount":
            accountDialog.open()
            break
        }
    }

    spacing: centered ? 22 : 14
    objectName: "launcher"
    Accessible.id: objectName

    Component.onCompleted: {
        Models.Launcher.refresh(false)
        Models.Launcher.loadModels(agentId)
    }
    onVisibleChanged: {
        if (visible) {
            Models.Launcher.refresh(false)
            promptInput.forceActiveFocus()
        }
    }
    onAgentIdChanged: Models.Launcher.loadModels(agentId)
    onInfoChanged: Models.Launcher.loadModels(agentId)

    Connections {
        target: Models.Launcher
        enabled: root.asked

        function onAccountErrorChanged() {
            if (Models.Launcher.addingAccount)
                return
            root.asked = false
            root.error = Models.Launcher.accountError
        }
    }
    Connections {
        target: Models.DesktopFiles

        function onDirectoryPicked(purpose, path) {
            if (purpose === "launcher" && root.visible)
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

    component Chip: Item {
        id: chip

        property bool lit: false
        property bool pressed: false
        property bool focused: false

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

    component Key: AbstractButton {
        id: key

        property string words: ""
        property string glyph: ""
        property color glyphColor: KodosiTheme.terminalInkMuted
        property color tint: KodosiTheme.terminalInk
        property real maximumWidth: 480
        property string kind: ""
        readonly property bool open: menu.visible && menu.kind === kind

        hoverEnabled: true
        activeFocusOnTab: true
        focusPolicy: Qt.TabFocus
        implicitHeight: 26
        implicitWidth: Math.max(30, Math.min(maximumWidth, keyRow.implicitWidth + (words.length > 0 ? 16 : 0)))
        scale: pressed ? 0.95 : 1
        Accessible.role: Accessible.ButtonMenu
        Accessible.description: words
        ToolTip.visible: hovered && !open
        ToolTip.text: Accessible.name
        ToolTip.delay: 600
        onClicked: root.showMenu(kind, key)

        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

        background: Chip {
            lit: key.hovered || key.open
            pressed: key.down
            focused: key.visualFocus
        }
        contentItem: Item {
            Row {
                id: keyRow

                anchors.centerIn: parent
                spacing: 5

                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: key.glyph.length > 0
                    width: key.words.length > 0 ? 12 : 13
                    height: width
                    name: key.glyph
                    strokeWidth: key.words.length > 0 ? 2.2 : 2.4
                    color: key.glyphColor
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: key.words.length > 0
                    width: Math.min(implicitWidth, key.maximumWidth - (key.glyph.length > 0 ? 58 : 40))
                    text: key.words
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                    color: key.tint
                    font: root.commandFont
                }
                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: key.words.length > 0
                    width: 8
                    height: 8
                    name: "chevron-down"
                    strokeWidth: 2.8
                    color: KodosiTheme.terminalInkMuted
                }
            }
        }
    }

    Item {
        id: preview

        Layout.alignment: root.centered ? Qt.AlignHCenter : Qt.AlignLeft
        Layout.preferredWidth: Math.min(560, root.width)
        implicitHeight: band.height + Math.max(root.centered ? 130 : 104, terminalBody.implicitHeight + 32)
        objectName: "launcher.preview"
        Accessible.id: objectName

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
                        ToolTip.text: modelData.installed ? title : qsTr("%1 is not on this computer").arg(title)
                        activeFocusOnTab: true
                        hoverEnabled: true
                        height: tabsRow.height
                        width: markRow.implicitWidth + (selected ? 24 : 16)
                        scale: pressed ? 0.96 : 1
                        clip: true

                        Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

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
                        background: Rectangle {
                            y: 2
                            height: parent.height - 4
                            radius: KodosiTheme.radiusSm
                            color: mark.hovered && !mark.selected ? "#2a221e" : "transparent"
                            border.width: mark.visualFocus ? 1.5 : 0
                            border.color: KodosiTheme.terminalAccent

                            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
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
                objectName: "launcher.place"

                KIcon { anchors.verticalCenter: parent.verticalCenter; width: 13; height: 13; name: "laptop"; strokeWidth: 1.9; color: KodosiTheme.terminalInkMuted }
                PlainLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.roomName.length > 0 ? qsTr("This computer") + "  ·" : qsTr("This computer")
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
            }
        }
        TapHandler {
            onTapped: eventPoint => {
                const point = commandLine.mapFromItem(preview, eventPoint.position)
                if (!commandLine.contains(point))
                    promptInput.forceActiveFocus()
            }
        }
        Item {
            id: terminalBody

            x: 16
            y: band.height + 14
            width: preview.width - 32
            implicitHeight: commandLine.implicitHeight

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

                    Key {
                        maximumWidth: 240
                        glyph: "folder"
                        glyphColor: "#5dafa6"
                        words: root.tilde(root.directory) === "~" ? "~" : Models.Sessions.folderName(root.directory)
                        kind: "folder"
                        objectName: "launcher.folder"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Folder")
                    }
                    Text {
                        height: 26
                        verticalAlignment: Text.AlignVCenter
                        text: "❯"
                        color: KodosiTheme.terminalAccent
                        font: root.commandFont
                        Accessible.ignored: true
                    }
                    Key {
                        visible: root.installed && root.info.accounts.length > 1 && !!root.plan.account
                        words: visible ? root.accountTitle(root.plan.account) : ""
                        tint: root.info ? KodosiTheme.mix(Identity.kindTint(root.info.agent), "#ffffff", 0.35) : KodosiTheme.terminalInk
                        kind: "account"
                        objectName: "launcher.account"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Account")
                    }
                    Key {
                        visible: root.installed && root.plan.modelArgument.length > 0
                        words: !visible ? "" : root.plan.model ? root.plan.model.label : root.plan.modelArgument
                        kind: "model"
                        objectName: "launcher.model"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Model")
                    }
                    Key {
                        visible: root.installed && root.plan.effortArgument.length > 0
                        words: {
                            if (!visible)
                                return ""
                            const effort = root.plan.efforts.find(entry => entry.argument === root.plan.effortArgument)
                            return effort ? root.effortLabel(effort) : root.plan.effortArgument
                        }
                        kind: "effort"
                        objectName: "launcher.effort"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Effort")
                    }
                    Key {
                        visible: root.hasOptions
                        glyph: "more"
                        kind: "options"
                        objectName: "launcher.options"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Model, effort and accounts")
                    }
                    Text {
                        visible: root.info !== null && !root.info.installed
                        height: 26
                        verticalAlignment: Text.AlignVCenter
                        text: root.info ? root.info.install : ""
                        textFormat: Text.PlainText
                        color: KodosiTheme.terminalInkMuted
                        font: root.commandFont
                    }
                }
                LauncherPrompt {
                    id: promptInput

                    visible: root.takesWords
                    x: commandLine.inline ? commandLine.rowEnd : 0
                    y: commandLine.inline ? commandLine.rowTop : keys.height + 6
                    width: commandLine.inline ? commandLine.width - commandLine.rowEnd : commandLine.width
                    placeholder: root.info ? qsTr("first prompt (optional)") : qsTr("command (optional)")
                    objectName: "launcher.prompt"
                    Accessible.id: objectName
                    Accessible.name: root.info ? qsTr("First prompt") : qsTr("Command")
                    onSubmitted: root.start()
                }
            }
        }
    }
    PlainLabel {
        Layout.alignment: root.centered ? Qt.AlignHCenter : Qt.AlignLeft
        Layout.preferredWidth: preview.width
        visible: root.error.length > 0
        text: root.error
        color: KodosiTheme.caution
        font.pixelSize: KodosiTheme.fontFootnote
        wrapMode: Text.WordWrap
    }
    RowLayout {
        Layout.alignment: root.centered ? Qt.AlignHCenter : Qt.AlignLeft
        spacing: 10

        KButton {
            objectName: "launcher.start"
            Accessible.id: objectName
            variant: KButton.Primary
            iconName: "arrow-right"
            iconTrailing: true
            enabled: root.ready && !Models.SessionActions.busy
            working: Models.SessionActions.busy
            text: !root.info ? qsTr("Open a terminal")
                : root.info.installed ? qsTr("Start %1").arg(root.label) : qsTr("Install %1").arg(root.label)
            onClicked: root.start()
        }
        KButton {
            objectName: "launcher.resume"
            Accessible.id: objectName
            variant: KButton.Secondary
            text: qsTr("Resume…")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Continue a saved Claude Code or Copilot CLI conversation")
            ToolTip.delay: 600
            onClicked: root.resumeRequested()
        }
    }
    KMenu {
        id: menu

        property string kind: ""

        objectName: "launcher.menu"
    }
    Component {
        id: menuChoice

        KMenuItem {
            property var entry: ({})

            onTriggered: root.pick(entry)
        }
    }
    Component {
        id: menuSeparator

        MenuSeparator {
            contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.alpha(KodosiTheme.hairline, 0.7) }
        }
    }
    Component {
        id: menuPart

        KMenu {}
    }
    KDialog {
        id: accountDialog

        objectName: "launcher.account.new"
        title: qsTr("New %1 account").arg(root.label)
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: {
            accountName.clear()
            standardButton(Dialog.Ok).text = qsTr("Add")
            standardButton(Dialog.Ok).enabled = Qt.binding(() => accountName.text.trim().length > 0)
            accountName.forceActiveFocus()
        }
        onAccepted: {
            root.error = ""
            root.asked = true
            Models.Launcher.addAccount(root.info.agent, accountName.text)
        }
        onClosed: promptInput.forceActiveFocus()

        ColumnLayout {
            spacing: 12

            PlainLabel {
                Layout.preferredWidth: 340
                text: qsTr("Kodosi makes a sign-in folder for it. %1 asks you to sign in when it first starts.").arg(root.label)
                color: KodosiTheme.inkMuted
                wrapMode: Text.WordWrap
            }
            KTextField {
                id: accountName

                Layout.fillWidth: true
                placeholderText: qsTr("Name")
                objectName: "launcher.account.name"
                Accessible.id: objectName
                Accessible.name: placeholderText
                onAccepted: {
                    if (text.trim().length > 0)
                        accountDialog.accept()
                }
            }
        }
    }
}
