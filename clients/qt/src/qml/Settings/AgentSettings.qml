pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    property string chosen: ""
    readonly property var catalog: Models.Launcher.agents
    readonly property var agents: catalog.length === 0 ? ["claude", "copilot"]
        : catalog.filter(entry => entry.installed || entry.agent === "claude" || entry.agent === "copilot").map(entry => entry.agent)
    readonly property string agent: {
        if (agents.indexOf(chosen) >= 0)
            return chosen
        const installed = catalog.filter(entry => entry.installed).map(entry => entry.agent)
        const current = Models.Launcher.choice.agent || ""
        return installed.indexOf(current) >= 0 ? current : installed[0] || agents[0] || "claude"
    }
    readonly property var info: catalog.find(entry => entry.agent === agent) || null
    readonly property bool provider: agent === "claude" || agent === "copilot"
    readonly property var installation: Models.ProviderFiles.installation
    readonly property var files: installation.files || []

    function inspect() {
        if (provider)
            Models.ProviderFiles.inspect(agent, Models.DesktopSettings.effectiveWorkingDirectory);
    }

    spacing: 18
    onVisibleChanged: {
        if (visible) {
            Models.Launcher.refresh(false);
            inspect();
        }
    }
    onAgentChanged: {
        if (visible)
            inspect();
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 12

        AgentMark {
            program: root.agent
            size: 40
            asleep: root.info !== null && !root.info.installed
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            PlainLabel { text: Models.DesktopSettings.agentLabel(root.agent); font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold }
            ShimmerText {
                Layout.fillWidth: true
                visible: root.provider
                text: Models.ProviderFiles.busy ? qsTr("Looking") : root.installation.executable || root.installation.message || qsTr("Kodosi opens these files. It does not change them.")
                active: Models.ProviderFiles.busy
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                elide: Text.ElideMiddle
            }
        }
        SegmentedPill {
            compact: true
            identifier: "settings.agents"
            options: root.agents.map(entry => ({ value: entry, label: Models.DesktopSettings.agentLabel(entry) }))
            currentValue: root.agent
            onActivated: value => root.chosen = value
        }
    }
    AgentDefaults {
        Layout.fillWidth: true
        visible: root.info !== null && root.info.installed === true
        info: root.info
    }
    ListGroup {
        Layout.fillWidth: true
        visible: root.provider && root.files.length > 0

        Repeater {
            model: root.files

            delegate: ListRow {
                id: fileRow

                required property var modelData

                iconName: "document"
                tint: fileRow.modelData.exists ? "#607fcc" : "#54433a"
                title: fileRow.modelData.label
                subtitle: fileRow.modelData.path

                Tag { visible: !fileRow.modelData.exists; text: qsTr("Not made yet") }
                KButton {
                    Accessible.id: objectName
                    compact: true
                    visible: fileRow.modelData.exists
                    objectName: "panel.settingsView.open" + "." + fileRow.modelData.label
                    text: qsTr("Open")

                    onClicked: Models.DesktopFiles.openPath(fileRow.modelData.path)
                }
            }
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        color: KodosiTheme.danger
        font.pixelSize: KodosiTheme.fontFootnote
        text: Models.ProviderFiles.error
        visible: root.provider && Models.ProviderFiles.error.length > 0
        wrapMode: Text.WordWrap
    }
}
