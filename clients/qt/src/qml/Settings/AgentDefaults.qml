pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ListGroup {
    id: root

    property var info: null
    readonly property string agent: info ? info.agent : ""
    readonly property string agentLabel: Models.DesktopSettings.agentLabel(agent)
    readonly property var plan: agent.length > 0 ? Models.Launcher.plans[agent] || null : null
    readonly property var account: plan ? plan.account || null : null
    readonly property var modelSet: agent.length > 0 ? Models.Launcher.models[agent] : undefined
    readonly property bool loadingModels: agent.length > 0 && Models.Launcher.loadingModels.indexOf(agent) >= 0
    readonly property bool listsModels: !modelSet || modelSet.models.length > 0
    readonly property string modelId: plan ? plan.modelId : ""
    readonly property string modelTitle: plan && plan.model ? plan.model.label : modelId.length > 0 ? modelId : qsTr("Default")
    readonly property var efforts: plan ? plan.efforts : []
    readonly property string effortId: plan ? plan.effortId : ""
    readonly property int effortLevel: efforts.findIndex(entry => entry.id === effortId)
    readonly property string permissions: plan ? plan.permissions : ""
    readonly property string output: plan ? plan.output : ""

    function loadModels() {
        if (visible && agent.length > 0)
            Models.Launcher.loadModels(agent)
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
    function permissionLabel(id) {
        return id === "unrestricted" ? qsTr("Unrestricted") : id === "auto" ? qsTr("Auto") : qsTr("Ask")
    }
    function permissionDetail(id) {
        if (id === "unrestricted")
            return qsTr("Never asks, and runs every command as you.")
        if (id !== "auto")
            return qsTr("Asks before it changes files or runs commands.")
        if (agent === "claude" || agent === "copilot")
            return qsTr("A safety check approves what is safe and asks you for the rest. The check uses tokens.")
        if (agent === "codex")
            return qsTr("Works in this folder on its own and asks you for the rest.")
        if (agent === "gemini")
            return qsTr("Edits files on its own and asks before it runs commands.")
        return qsTr("Accepts its own actions, except what you denied.")
    }
    function outputLabel(id) {
        return id === "quiet" ? qsTr("Quiet") : id === "detailed" ? qsTr("Detailed") : qsTr("Standard")
    }
    function outputDetail(id) {
        return id === "quiet" ? qsTr("Shows less of its work.") : id === "detailed" ? qsTr("Shows each step of its work in full.")
            : qsTr("Shows its work as the agent is set up.")
    }

    footer: qsTr("New terminals start with these. What you choose in the launcher changes them too.")
    onVisibleChanged: loadModels()
    onAgentChanged: loadModels()

    component ModeSetting: Item {
        id: setting

        property string title: ""
        property string detail: ""
        property string iconName: ""
        property color tint: KodosiTheme.accent
        property var options: []
        property string current: ""
        property string identifier: ""

        signal picked(string value)

        width: parent ? parent.width : implicitWidth
        implicitHeight: Math.max(48, words.implicitHeight + 20)
        Accessible.role: Accessible.Grouping
        Accessible.name: title

        Rectangle {
            visible: setting.Positioner.index > 0
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 14
            anchors.rightMargin: 14
            height: 1
            color: KodosiTheme.alpha(KodosiTheme.hairline, 0.6)
        }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 12

            IconTile {
                iconName: setting.iconName
                tint: setting.tint
                size: 26
            }
            ColumnLayout {
                id: words

                Layout.fillWidth: true
                spacing: 2

                PlainLabel {
                    Layout.fillWidth: true
                    text: setting.title
                    font.weight: Font.Medium
                    elide: Text.ElideRight
                }
                PlainLabel {
                    Layout.fillWidth: true
                    text: setting.detail
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontCaption
                    wrapMode: Text.WordWrap
                }
            }
            SegmentedPill {
                compact: true
                identifier: setting.identifier
                options: setting.options
                currentValue: setting.current
                onActivated: value => {
                    if (value !== setting.current)
                        setting.picked(value)
                }
            }
        }
    }

    ListRow {
        visible: root.info !== null && root.info.accounts.length > 1
        title: qsTr("Account")
        subtitle: root.account ? root.account.identity || qsTr("Signs in when it starts") : ""
        leading: AccountMark {
            size: 26
            initial: accountPicker.text.charAt(0)
            key: root.account ? root.account.folder || root.account.command : ""
            signedIn: !!root.account && !!root.account.identity
            tint: root.info && root.account
                ? Identity.accountTint(root.agent, root.info.accounts.findIndex(entry => entry.name === root.account.name))
                : Identity.kindTint(root.agent)
        }

        KPicker {
            id: accountPicker

            objectName: "settings.agents.account"
            Accessible.name: qsTr("Account")
            values: (root.info ? root.info.accounts : []).map(entry => ({ value: entry.name,
                label: Models.Launcher.accountTitle(entry.name, root.info.program) }))
            current: root.account ? root.account.name : ""
            onPicked: value => Models.Launcher.choose("account", root.agent, value)
        }
    }
    ListRow {
        visible: root.listsModels || root.efforts.length > 0
        title: qsTr("Model")
        iconName: "sparkle"
        tint: Identity.kindTint(root.agent)

        EffortBars {
            visible: root.effortLevel >= 0
            level: root.effortLevel
            count: root.efforts.length
            barHeight: 10
        }
        KButton {
            id: modelButton

            compact: true
            iconName: "chevron-down"
            iconTrailing: true
            working: !root.modelSet && root.loadingModels
            text: root.modelTitle
            objectName: "settings.agents.model"
            Accessible.id: objectName
            Accessible.name: qsTr("Model and effort")
            Accessible.description: root.effortLevel >= 0 ? Models.DesktopSettings.effortLabel(root.effortId) : ""
            onClicked: {
                if (modelPopover.visible)
                    modelPopover.close()
                else
                    modelPopover.open()
            }

            LauncherDropdown {
                id: modelPopover

                x: modelButton.width - width
                onOpened: modelList.reset()
                contentItem: Column {
                    width: 290

                    PickerList {
                        id: modelList

                        rows: root.modelRows()
                        listWidth: 290
                        objectName: "settings.agents.list.model"
                        Accessible.id: objectName
                        onPicked: id => {
                            Models.Launcher.choose("model", root.agent, id === "menu.default" ? "" : id)
                            modelPopover.close()
                        }
                        onClosed: modelPopover.close()
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
                        levels: root.efforts.map(entry => ({ id: entry.id, label: Models.DesktopSettings.effortLabel(entry.id) }))
                        current: root.effortId
                        objectName: "settings.agents.effort"
                        Accessible.id: objectName
                        onPicked: effort => {
                            Models.Launcher.choose("effort", root.agent, effort)
                            modelPopover.close()
                        }
                    }
                }
            }
        }
    }
    ModeSetting {
        visible: root.info !== null && (root.info.permissions || []).length > 1
        title: qsTr("Permissions")
        detail: root.permissionDetail(root.permissions)
        iconName: root.permissions === "unrestricted" ? "warning" : root.permissions === "auto" ? "check" : "hand"
        tint: root.permissions === "unrestricted" ? "#d9a441" : Identity.kindTint(root.agent)
        options: (root.info ? root.info.permissions || [] : []).map(id => ({ value: id, label: root.permissionLabel(id) }))
        current: root.permissions
        identifier: "settings.agents.permissions"
        onPicked: value => Models.Launcher.choose("permissions", root.agent, value)
    }
    ModeSetting {
        visible: root.info !== null && (root.info.outputs || []).length > 0
        title: qsTr("Verbosity")
        detail: root.outputDetail(root.output)
        iconName: "sliders"
        tint: "#6b6560"
        options: (root.info ? root.info.outputs || [] : []).map(id => ({ value: id, label: root.outputLabel(id) }))
        current: root.output
        identifier: "settings.agents.output"
        onPicked: value => Models.Launcher.choose("output", root.agent, value)
    }
}
