pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

KMenu {
    id: root

    required property var session
    required property string sessionId
    property string identifier: ""
    property bool agentOnly: false
    readonly property bool shareable: session.kind === "local" && session.isOwner === true
    readonly property bool changesAgent: agentChanges.shown

    signal canvasRequested(string section)
    signal shareRequested()
    signal inspectRequested()

    readonly property var agents: !root.agentOnly && root.session.atPrompt === true ? Models.Launcher.agents.filter(agent => agent.installed) : []

    cascade: true
    onAboutToShow: agentChanges.loadModels()

    PlainLabel {
        visible: root.agents.length > 0
        height: visible ? implicitHeight : 0
        leftPadding: 10
        topPadding: 4
        bottomPadding: 2
        text: qsTr("Start an agent")
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    Instantiator {
        model: root.agents
        onObjectAdded: (index, item) => root.insertItem(index + 1, item)
        onObjectRemoved: (index, item) => root.removeItem(item)
        delegate: KMenuItem {
            required property var modelData

            text: Models.DesktopSettings.agentLabel(modelData.agent)
            agent: modelData.agent
            objectName: root.identifier + ".start." + modelData.agent
            onTriggered: Models.SessionActions.startAgent(root.sessionId, modelData.agent)
        }
    }
    MenuSeparator {
        visible: root.agents.length > 0
        height: visible ? implicitHeight : 0
        contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
    }
    PlainLabel {
        visible: agentChanges.shown
        height: visible ? implicitHeight : 0
        leftPadding: 10
        topPadding: 4
        bottomPadding: 2
        text: agentChanges.shown ? Models.DesktopSettings.agentLabel(agentChanges.agent) : ""
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    AgentChoiceMenu {
        title: qsTr("Model")
        objectName: root.identifier + ".agent.model"
        available: agentChanges.shown && agentChanges.live.model === true && agentChanges.models !== null
        hint: agentChanges.modelHint
        options: agentChanges.modelOptions
        current: agentChanges.model
        note: agentChanges.keepingNote
        onPicked: id => Models.SessionActions.changeAgent(root.sessionId, { model: id })
    }
    AgentChoiceMenu {
        title: qsTr("Effort")
        objectName: root.identifier + ".agent.effort"
        available: agentChanges.shown && agentChanges.live.effort === true && agentChanges.effortOptions.length > 0
        hint: agentChanges.effortHint
        options: agentChanges.effortOptions
        current: agentChanges.effort
        note: agentChanges.keepingNote
        onPicked: id => Models.SessionActions.changeAgent(root.sessionId, { effort: id })
    }
    KMenuItem {
        text: agentChanges.fastTitle
        checkable: true
        checked: agentChanges.fast
        visible: agentChanges.shown && agentChanges.live.fast === true
        objectName: root.identifier + ".agent.fast"
        onToggled: {
            const wanted = checked;
            checked = Qt.binding(() => agentChanges.fast);
            Models.SessionActions.changeAgent(root.sessionId, { fast: wanted });
        }
    }
    AgentChoiceMenu {
        title: qsTr("Permissions")
        objectName: root.identifier + ".agent.permissions"
        available: agentChanges.shown && (agentChanges.live.permissions || []).length > 0
        hint: agentChanges.permissionsHint
        options: agentChanges.permissionOptions
        current: agentChanges.permissions
        note: agentChanges.turnNote
        onPicked: id => Models.SessionActions.changeAgent(root.sessionId, { permissions: id })
    }
    AgentChoiceMenu {
        title: qsTr("Verbosity")
        objectName: root.identifier + ".agent.output"
        available: agentChanges.shown && agentChanges.output.length > 0
        hint: agentChanges.outputHint
        options: agentChanges.outputOptions
        current: agentChanges.output
    }
    MenuSeparator {
        visible: agentChanges.shown && !root.agentOnly
        height: visible ? implicitHeight : 0
        contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
    }
    PlainLabel {
        visible: !root.agentOnly && root.session.canControl === true
        height: visible ? implicitHeight : 0
        leftPadding: 10
        topPadding: 4
        bottomPadding: 2
        text: qsTr("Canvas")
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    KMenuItem {
        text: qsTr("Open Browser")
        iconName: "globe"
        visible: !root.agentOnly && root.session.canControl === true
        objectName: root.identifier + ".browser"
        onTriggered: {
            Models.Canvas.openBrowser(root.sessionId);
            root.canvasRequested("browser");
        }
    }
    KMenuItem {
        text: qsTr("Add a file…")
        iconName: "files"
        visible: !root.agentOnly && root.session.canControl === true
        objectName: root.identifier + ".addFile"
        onTriggered: Models.DesktopFiles.requestFiles(root.sessionId)
    }
    MenuSeparator {
        visible: !root.agentOnly && root.session.canControl === true
        height: visible ? implicitHeight : 0
        contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
    }
    PlainLabel {
        visible: !root.agentOnly
        height: visible ? implicitHeight : 0
        leftPadding: 10
        topPadding: 4
        bottomPadding: 2
        text: qsTr("Terminal")
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    KMenuItem {
        text: qsTr("Share…")
        iconName: "share"
        visible: !root.agentOnly && root.shareable
        objectName: root.identifier + ".share"
        onTriggered: root.shareRequested()
    }
    KMenuItem {
        text: qsTr("Details")
        iconName: "info"
        visible: !root.agentOnly
        objectName: root.identifier + ".details"
        onTriggered: root.inspectRequested()
    }
    KMenuItem {
        text: qsTr("Remove from project")
        iconName: "leave"
        destructive: true
        visible: !root.agentOnly && !!root.session.projectId && root.session.isOwner === true
        objectName: root.identifier + ".removeFromProject"
        onTriggered: Models.SessionActions.attachProject(root.sessionId, "")
    }

    QtObject {
        id: agentChanges

        readonly property string agent: root.session.program || ""
        readonly property var live: root.session.programLive || ({})
        readonly property var settings: root.session.programSettings || ({})
        readonly property var pending: root.session.programPending || ({})
        readonly property bool changesModes: agent.length > 0 && (live.permissions || []).length > 0
        readonly property bool local: agent.length > 0 && (live.model === true || live.effort === true)
            && root.session.kind === "local" && Models.Launcher.computer.length === 0
        readonly property var models: local ? Models.Launcher.models[agent] || null : null
        readonly property string output: settings.output ?? ""
        readonly property bool shown: root.session.canControl === true
            && (models !== null || changesModes || output.length > 0 || live.fast === true)
        readonly property string model: pending.model ?? settings.model ?? ""
        readonly property string effort: pending.model !== undefined ? pending.effort ?? "" : pending.effort ?? settings.effort ?? ""
        readonly property bool fast: pending.fast ?? settings.fast ?? false
        readonly property string permissions: pending.permissions ?? settings.permissions ?? ""
        readonly property var modelOptions: (models ? models.models || [] : []).map(option => ({ id: option.id, label: option.label }))
        readonly property var effortOptions: {
            const offer = models ? models.models || [] : []
            const allowed = (offer.find(option => option.id === model) || {}).efforts || []
            return (models ? models.efforts || [] : []).filter(option => allowed.length === 0 || allowed.includes(option.id))
                .map(option => ({ id: option.id, label: Models.DesktopSettings.effortLabel(option.id) }))
        }
        readonly property var knownPermissions: changesModes
            ? (Models.Launcher.agents.find(info => info.agent === agent) || {}).permissions || [] : []
        readonly property var permissionOptions: ["ask", "auto", "unrestricted"]
            .filter(id => knownPermissions.includes(id) || (live.permissions || []).includes(id) || id === permissions)
            .map(id => ({ id: id, label: permissionLabel(id), enabled: (live.permissions || []).includes(id),
                detail: (live.permissions || []).includes(id) ? "" : qsTr("Only at start") }))
        readonly property var knownOutputs: output.length > 0
            ? (Models.Launcher.agents.find(info => info.agent === agent) || {}).outputs || [] : []
        readonly property var outputOptions: ["quiet", "standard", "detailed"]
            .filter(id => knownOutputs.includes(id) || id === output)
            .map(id => ({ id: id, label: outputLabel(id), enabled: false, detail: id === output ? "" : qsTr("Only at start") }))
        readonly property string modelHint: shownValue(modelLabel(model), pending.model !== undefined)
        readonly property string effortHint: shownValue(effort.length > 0 ? Models.DesktopSettings.effortLabel(effort) : "",
            pending.effort !== undefined)
        readonly property string fastTitle: shownValue(qsTr("Fast mode"), pending.fast !== undefined)
        readonly property string permissionsHint: shownValue(permissionLabel(permissions), pending.permissions !== undefined)
        readonly property string outputHint: outputLabel(output)
        readonly property string keepingNote: note(live.keeps === true)
        readonly property string turnNote: note(false)

        function outputLabel(id: string): string {
            return id === "quiet" ? qsTr("Quiet") : id === "detailed" ? qsTr("Detailed") : id === "standard" ? qsTr("Standard") : ""
        }

        function loadModels(): void {
            if (local)
                Models.Launcher.loadModels(agent)
        }
        function modelLabel(id: string): string {
            if (id.length === 0)
                return qsTr("Default")
            return ((models ? models.models || [] : []).find(option => option.id === id) || { label: id }).label
        }
        function permissionLabel(id: string): string {
            return id === "unrestricted" ? qsTr("Unrestricted") : id === "auto" ? qsTr("Auto") : id === "ask" ? qsTr("Ask") : ""
        }
        function shownValue(value: string, waits: bool): string {
            return waits ? qsTr("%1, after this turn").arg(value) : value
        }
        function note(keeps: bool): string {
            if (!shown)
                return ""
            const label = Models.DesktopSettings.agentLabel(agent)
            const busy = root.session.working === true || root.session.asks === true
            return [busy ? qsTr("%1 changes when it finishes its turn.").arg(label) : "",
                keeps ? qsTr("%1 also keeps the model, effort and fast mode the next time it starts.").arg(label) : ""]
                .filter(text => text.length > 0).join(" ")
        }
    }
}
