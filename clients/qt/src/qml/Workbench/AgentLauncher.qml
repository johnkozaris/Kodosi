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
    readonly property string note: !info ? ""
        : !info.installed ? qsTr("%1 is not on this computer yet").arg(Identity.kindLabel(info.agent))
        : !account ? "" : account.identity || (account.command === account.name ? "" : qsTr("signs in when it starts"))

    signal resumeRequested

    function tilde(path) {
        return path === home ? "~" : path.startsWith(home + "/") ? "~" + path.slice(home.length) : path
    }
    function folderName(path) {
        const parts = path.split("/").filter(part => part.length > 0)
        return parts.length > 0 ? parts[parts.length - 1] : "/"
    }
    function start() {
        if (info && line.length === 0)
            return
        Models.SessionActions.start(directory, line)
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
                detail: entry.identity || (entry.command === entry.name ? "" : qsTr("Signs in when it starts")) }))
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

    Component.onCompleted: Models.Launcher.refresh(false)
    onAgentIdChanged: {
        showsEffort = false
        Models.Launcher.loadModels(agentId)
    }
    onInfoChanged: Models.Launcher.loadModels(agentId)

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

    component Token: AbstractButton {
        id: token

        property string words: ""
        property string badge: ""
        property bool slot: false
        property bool loading: false
        property color tint: KodosiTheme.ink
        property real maximumWidth: 480

        hoverEnabled: true
        activeFocusOnTab: true
        implicitHeight: 26
        implicitWidth: Math.min(maximumWidth, tokenRow.implicitWidth + 12)
        scale: pressed ? 0.96 : 1
        Accessible.role: Accessible.ButtonMenu
        Accessible.description: words + (badge.length > 0 ? " " + badge : "")

        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

        background: Item {
            Rectangle {
                anchors.fill: parent
                radius: KodosiTheme.radiusXs
                color: KodosiTheme.alpha(KodosiTheme.ink, token.down || menu.opened && menu.anchor === token ? 0.1 : token.hovered ? 0.06 : 0)
                border.width: token.visualFocus ? 1.5 : 0
                border.color: KodosiTheme.accentStrong
            }
            Shape {
                anchors.fill: parent
                visible: token.slot
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    strokeColor: KodosiTheme.alpha(KodosiTheme.inkFaint, token.hovered ? 0.75 : 0.4)
                    strokeWidth: 1
                    strokeStyle: ShapePath.DashLine
                    dashPattern: [3, 3]
                    fillColor: "transparent"
                    PathRectangle { x: 0.5; y: 0.5; width: token.width - 1; height: token.height - 1; radius: KodosiTheme.radiusXs }
                }
            }
        }
        contentItem: Item {
            Row {
                id: tokenRow
                anchors.verticalCenter: parent.verticalCenter
                x: 6
                spacing: 5

                ShimmerText {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: token.loading
                    text: token.words
                    active: true
                    color: KodosiTheme.inkFaint
                    font: root.commandFont
                }
                TypedText {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !token.loading
                    width: Math.min(implicitWidth, token.maximumWidth - 40)
                    target: token.words
                    color: token.slot ? KodosiTheme.inkFaint : token.tint
                    font: root.commandFont
                }
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: token.badge.length > 0
                    width: badgeText.implicitWidth + 12
                    height: 18
                    radius: 9
                    color: KodosiTheme.alpha(token.tint, 0.16)

                    PlainLabel {
                        id: badgeText
                        anchors.centerIn: parent
                        text: token.badge
                        color: token.tint
                        font.pixelSize: KodosiTheme.fontCaption
                        font.weight: Font.DemiBold
                    }
                }
                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8
                    height: 8
                    name: "chevron-down"
                    strokeWidth: 2.6
                    color: KodosiTheme.alpha(KodosiTheme.inkFaint, token.hovered ? 1 : 0.55)
                }
            }
        }
    }

    Item {
        id: dock

        readonly property var items: [{ id: "", kind: "shell", installed: true }]
            .concat(root.agents.filter(entry => entry.installed || root.showsAll || entry.agent === root.agentId)
                .map(entry => ({ id: entry.agent, kind: entry.agent, installed: entry.installed })))
        readonly property int hidden: root.agents.filter(entry => !entry.installed && entry.agent !== root.agentId).length
        readonly property Item current: marks.count > 0 ? marks.itemAt(Math.max(0, items.findIndex(entry => entry.id === root.agentId))) : null

        Layout.alignment: root.welcome ? Qt.AlignHCenter : Qt.AlignLeft
        implicitWidth: dockRow.width + 6
        implicitHeight: 38
        objectName: "launcher.agents"
        Accessible.role: Accessible.PageTabList
        Accessible.name: qsTr("Agents")

        Well {
            anchors.fill: parent
            radius: height / 2
        }
        Raised {
            x: dock.current ? dockRow.x + dock.current.x : 3
            y: 3
            width: dock.current ? dock.current.width : 0
            height: dock.height - 6
            radius: height / 2

            Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
            Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
        }
        Row {
            id: dockRow
            x: 3
            y: 3
            height: dock.height - 6
            spacing: 2

            Repeater {
                id: marks
                model: dock.items

                AbstractButton {
                    id: mark

                    required property var modelData
                    readonly property bool selected: modelData.id === root.agentId
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
                    height: dockRow.height
                    width: markRow.implicitWidth + (selected ? 20 : 14)
                    clip: true

                    Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                    onSelectedChanged: {
                        if (selected && !KodosiTheme.reduceMotion)
                            squash.restart()
                    }
                    SequentialAnimation {
                        id: squash
                        NumberAnimation { target: mark; property: "bounce"; to: 0.84; duration: 90; easing.type: Easing.OutQuad }
                        NumberAnimation { target: mark; property: "bounce"; to: 1; duration: KodosiTheme.motionSoft; easing.type: Easing.OutBack; easing.overshoot: 2.2 }
                    }

                    contentItem: Item {
                        Row {
                            id: markRow
                            anchors.verticalCenter: parent.verticalCenter
                            x: 7
                            spacing: 7

                            AgentMark {
                                anchors.verticalCenter: parent.verticalCenter
                                program: mark.modelData.kind === "shell" ? "" : mark.modelData.kind
                                asleep: !mark.modelData.installed
                                size: 20
                                scale: mark.bounce
                            }
                            PlainLabel {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: mark.selected
                                text: mark.title
                                color: mark.modelData.installed ? KodosiTheme.ink : KodosiTheme.inkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                                font.weight: Font.DemiBold
                            }
                        }
                    }
                    background: Rectangle {
                        radius: height / 2
                        color: "transparent"
                        border.width: mark.visualFocus ? 1.5 : 0
                        border.color: KodosiTheme.accentStrong
                    }
                    onClicked: Models.Launcher.chooseAgent(modelData.id)
                }
            }
            AbstractButton {
                id: more

                visible: dock.hidden > 0
                objectName: "launcher.more"
                Accessible.id: objectName
                Accessible.name: qsTr("More agents")
                ToolTip.visible: hovered
                ToolTip.text: root.showsAll ? qsTr("Hide agents that are not on this computer") : qsTr("Show %1 more agents").arg(dock.hidden)
                activeFocusOnTab: true
                hoverEnabled: true
                width: 30
                height: dockRow.height
                onClicked: root.showsAll = !root.showsAll

                contentItem: Item {
                    KIcon {
                        anchors.centerIn: parent
                        width: 13
                        height: 13
                        name: "plus"
                        strokeWidth: 2.2
                        color: more.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                        rotation: root.showsAll ? 45 : 0

                        Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack } }
                    }
                }
                background: Item {}
            }
        }
    }

    Item {
        id: preview

        Layout.alignment: root.welcome ? Qt.AlignHCenter : Qt.AlignLeft
        Layout.preferredWidth: Math.min(540, root.width)
        implicitHeight: Math.max(root.welcome ? 196 : 150, previewColumn.implicitHeight + 28)
        objectName: "launcher.preview"
        Accessible.id: objectName

        HoverHandler { id: previewHover }
        TapHandler { onTapped: root.start() }

        Rectangle {
            anchors.fill: parent
            radius: KodosiTheme.radiusXl
            color: KodosiTheme.alpha(KodosiTheme.accent, previewHover.hovered ? 0.07 : 0)

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
        }
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                strokeColor: previewHover.hovered ? KodosiTheme.alpha(KodosiTheme.accent, 0.7) : KodosiTheme.alpha(KodosiTheme.inkFaint, 0.5)
                strokeWidth: 1.2
                strokeStyle: ShapePath.DashLine
                dashPattern: [4, 4]
                fillColor: "transparent"
                PathRectangle { x: 0.6; y: 0.6; width: preview.width - 1.2; height: preview.height - 1.2; radius: KodosiTheme.radiusXl }
            }
        }
        RowLayout {
            x: 18
            y: 14
            width: preview.width - 36
            spacing: 6

            Repeater {
                model: 3
                Rectangle { width: 7; height: 7; radius: 3.5; color: KodosiTheme.alpha(KodosiTheme.inkFaint, 0.35) }
            }
            Item { Layout.fillWidth: true }
            Token {
                id: folderToken
                words: root.folderName(root.directory)
                objectName: "launcher.folder"
                Accessible.id: objectName
                Accessible.name: qsTr("Folder")
                onClicked: root.openMenu("folder", folderToken)
            }
        }
        ColumnLayout {
            id: previewColumn
            x: 18
            width: preview.width - 36
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 12
            spacing: 8

            Flow {
                Layout.fillWidth: true
                spacing: 4

                Item {
                    width: 16
                    height: 26
                    KIcon { anchors.centerIn: parent; width: 13; height: 13; name: "prompt"; strokeWidth: 2.8; color: KodosiTheme.inkFaint }
                }
                Token {
                    id: accountToken
                    visible: root.info !== null && root.info.installed && root.account !== null
                    words: !root.account ? "" : root.account.command === root.account.name ? root.account.name : root.info.program
                    badge: !root.account || root.account.command === root.account.name ? "" : root.account.name
                    tint: root.info ? KodosiTheme.mix(Identity.kindTint(root.info.agent), KodosiTheme.ink, KodosiTheme.isDark ? 0.45 : 0.2) : KodosiTheme.ink
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
                KIconButton {
                    visible: root.info !== null && root.info.installed && root.efforts.length > 0
                        && root.effortArgument.length === 0 && !root.showsEffort
                    glyph: "more"
                    size: 26
                    objectName: "launcher.moreChoices"
                    Accessible.id: objectName
                    Accessible.name: qsTr("More choices")
                    onClicked: root.showsEffort = true
                }
                TypedText {
                    visible: root.info !== null && !root.info.installed
                    height: 26
                    verticalAlignment: Text.AlignVCenter
                    target: root.info ? root.info.install : ""
                    color: KodosiTheme.inkMuted
                    font: root.commandFont
                }
                Item {
                    width: 12
                    height: 26
                    CursorBlock { anchors.centerIn: parent; width: 9; height: 18; blinks: true }
                }
            }
            PlainLabel {
                Layout.fillWidth: true
                visible: root.note.length > 0
                text: "# " + root.note
                color: KodosiTheme.inkFaint
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontFootnote
                elide: Text.ElideMiddle
            }
        }
    }

    ColumnLayout {
        Layout.alignment: Qt.AlignHCenter
        visible: root.welcome
        spacing: 6

        PlainLabel {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Start with a terminal")
            font.pixelSize: KodosiTheme.fontTitle
            font.weight: Font.DemiBold
        }
        PlainLabel {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("It runs on this computer. Share it when you want company.")
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
        }
    }

    RowLayout {
        Layout.alignment: root.welcome ? Qt.AlignHCenter : Qt.AlignLeft
        spacing: 10

        KButton {
            objectName: "launcher.start"
            Accessible.id: objectName
            variant: KButton.Primary
            large: true
            iconName: "arrow-right"
            iconTrailing: true
            enabled: !root.info || root.line.length > 0
            text: !root.info ? qsTr("Open a terminal")
                : root.info.installed ? qsTr("Start %1").arg(Identity.kindLabel(root.info.agent))
                : qsTr("Install %1").arg(Identity.kindLabel(root.info.agent))
            onClicked: root.start()
        }
        KButton {
            objectName: "launcher.resume"
            Accessible.id: objectName
            variant: KButton.Ghost
            iconName: "history"
            text: qsTr("Resume a conversation")
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
                    text: root.info ? qsTr("%1 asks you to sign in when it starts.").arg(Identity.kindLabel(root.info.agent)) : ""
                    color: KodosiTheme.inkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                    wrapMode: Text.WordWrap
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
