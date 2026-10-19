pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property var details: Models.Dosi.details
    readonly property var setup: details.setup || ({})
    readonly property var installed: details.agents || []
    readonly property string agent: setup.agent || installed[0] || ""
    readonly property var info: Models.Launcher.agents.find(entry => entry.agent === agent) || null
    readonly property var modelSet: agent.length > 0 ? Models.Launcher.models[agent] : undefined
    readonly property var efforts: effortsFor(setup.model)
    readonly property var instructions: Models.Dosi.instructions
    property string draft: ""
    property bool editing: false
    readonly property bool changed: instructions.loaded === true && draft !== instructions.text

    function effortsFor(id) {
        if (!modelSet)
            return []
        const allowed = ((modelSet.models || []).find(entry => entry.id === id) || {}).efforts || []
        return (modelSet.efforts || []).filter(entry => allowed.length === 0 || allowed.indexOf(entry.id) >= 0)
    }
    function chooseModel(id) {
        const kept = effortsFor(id).some(entry => entry.id === setup.effort)
        choose({ model: id, effort: (kept || !modelSet) ? setup.effort || "" : "" })
    }

    function choose(next) {
        Models.Dosi.configure(Object.assign({ agent: agent, account: setup.account || "", model: setup.model || "", effort: setup.effort || "" }, next))
    }

    spacing: 22
    onVisibleChanged: {
        if (!visible)
            return
        Models.Launcher.refresh()
        Models.Dosi.loadInstructions()
        if (agent.length > 0)
            Models.Launcher.loadModels(agent)
    }
    onAgentChanged: {
        if (visible && agent.length > 0)
            Models.Launcher.loadModels(agent)
    }

    Connections {
        function onInstructionsChanged() {
            if (!root.changed || Models.Dosi.instructions.saving === false)
                root.draft = Models.Dosi.instructions.text || ""
        }

        target: Models.Dosi
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 14

        DosiFace {
            size: 52
            mood: Models.DesktopState.dosiOn ? Models.Dosi.mood : "off"
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 4

            PlainLabel {
                text: "Dosi"
                font.pixelSize: KodosiTheme.fontTitle
                font.weight: Font.DemiBold
            }
            PlainLabel {
                text: qsTr("Your agent for your agents")
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
            }
        }
        SegmentedPill {
            visible: Models.DesktopState.dosiOn && root.installed.length > 1
            compact: true
            identifier: "settings.dosi.agent"
            options: root.installed.map(entry => ({ value: entry, label: Models.DesktopSettings.agentLabel(entry) }))
            currentValue: root.agent
            onActivated: value => Models.Dosi.configure({ agent: value })
        }
    }
    ListGroup {
        Layout.fillWidth: true

        ListRow {
            iconName: "eye"
            tint: "#d97757"
            title: qsTr("Use Dosi")
            subtitle: qsTr("Off removes Dosi from this computer.")

            KSwitch {
                Accessible.id: objectName
                Accessible.name: qsTr("Use Dosi")
                checked: Models.DesktopState.dosiOn
                objectName: "settings.dosi.on"

                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => Models.DesktopState.dosiOn);
                    if (!wanted && Models.Dosi.busy)
                        Models.Dosi.stop();
                    Models.DesktopState.dosiOn = wanted;
                }
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.DesktopState.dosiOn && root.agent.length > 0
        footer: qsTr("Dosi works in its own folder with the account that you choose here.")

        ListRow {
            title: Models.DesktopSettings.agentLabel(root.agent)
            subtitle: qsTr("Account")
            leading: AgentMark { program: root.agent; size: 28 }

            Picker {
                objectName: "settings.dosi.account"
                values: [{ value: "", label: qsTr("Default") }].concat((root.info ? root.info.accounts : [])
                    .filter(entry => entry.name !== "default")
                    .map(entry => ({ value: entry.name, label: entry.identity || entry.name })))
                current: root.setup.account || ""
                onPicked: value => root.choose({ account: value })
            }
        }
        ListRow {
            title: qsTr("Model")
            iconName: "cube"
            tint: "#8660d9"

            Picker {
                objectName: "settings.dosi.model"
                values: [{ value: "", label: qsTr("The agent's choice") }].concat((root.modelSet ? root.modelSet.models : [])
                    .map(entry => ({ value: entry.id, label: entry.label })))
                current: root.setup.model || ""
                onPicked: value => root.chooseModel(value)
            }
        }
        ListRow {
            visible: root.efforts.length > 0
            title: qsTr("Effort")
            iconName: "sparkle"
            tint: "#4f9bb0"

            Picker {
                objectName: "settings.dosi.effort"
                values: [{ value: "", label: qsTr("The agent's choice") }].concat(root.efforts
                    .map(entry => ({ value: entry.id, label: entry.id === "xhigh" ? qsTr("Extra high") : entry.id.charAt(0).toUpperCase() + entry.id.slice(1) })))
                current: root.setup.effort || ""
                onPicked: value => root.choose({ effort: value })
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.DesktopState.dosiOn && root.agent.length === 0
        footer: qsTr("Dosi runs on Claude Code or Codex. Install one, then come back.")

        Repeater {
            model: Models.Launcher.agents.filter(entry => entry.agent === "claude" || entry.agent === "codex")

            ListRow {
                id: offer

                required property var modelData

                title: Models.DesktopSettings.agentLabel(modelData.agent)
                subtitle: modelData.install || ""
                leading: AgentMark { program: offer.modelData.agent; size: 28; asleep: true }

                KButton {
                    compact: true
                    text: qsTr("Copy")
                    onClicked: Models.DesktopFiles.copyText(offer.modelData.install)
                }
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.DesktopState.dosiOn
        footer: root.editing ? qsTr("Dosi reads these at the start of each conversation, on all your computers.") : ""

        ListRow {
            title: qsTr("Instructions")
            subtitle: root.instructions.edited === true ? qsTr("Yours") : qsTr("Standard")
            iconName: "document"
            tint: "#4f9bb0"

            RowLayout {
                spacing: 6

                KButton {
                    visible: root.editing && root.instructions.edited === true
                    compact: true
                    variant: KButton.Ghost
                    text: qsTr("Use the standard ones")
                    enabled: root.instructions.saving !== true
                    objectName: "settings.dosi.instructions.reset"
                    Accessible.id: objectName
                    onClicked: Models.Dosi.resetInstructions()
                }
                KButton {
                    visible: root.editing && root.changed
                    compact: true
                    variant: KButton.Primary
                    text: qsTr("Save")
                    enabled: root.instructions.saving !== true
                    objectName: "settings.dosi.instructions.save"
                    Accessible.id: objectName
                    onClicked: Models.Dosi.saveInstructions(root.draft)
                }
                KButton {
                    visible: !(root.editing && root.changed)
                    compact: true
                    text: root.editing ? qsTr("Done") : qsTr("Edit")
                    enabled: root.instructions.loaded === true
                    objectName: "settings.dosi.instructions.edit"
                    Accessible.id: objectName
                    onClicked: root.editing = !root.editing
                }
            }
        }
    }
    Item {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(420, Math.max(240, editor.contentHeight + 22))
        visible: Models.DesktopState.dosiOn && root.editing
        opacity: unfold.progress
        transform: Arrive { id: unfold; rise: 6 }
        onVisibleChanged: {
            if (visible)
                unfold.play()
        }

        Well {
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
        }
        ScrollView {
            id: editor
            anchors.fill: parent
            anchors.margins: 10

            TextArea {
                text: root.draft
                enabled: root.instructions.loaded === true
                color: KodosiTheme.ink
                selectionColor: KodosiTheme.accent
                selectedTextColor: KodosiTheme.accentInk
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontFootnote
                wrapMode: TextEdit.Wrap
                background: null
                objectName: "settings.dosi.instructions"
                Accessible.id: objectName
                Accessible.name: qsTr("Instructions")
                onTextChanged: root.draft = text
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.DesktopState.dosiOn

        ListRow {
            title: qsTr("Dosi's folder")
            subtitle: root.details.folder || ""
            iconName: "folder"
            tint: "#d9a441"

            KButton {
                compact: true
                text: qsTr("Open")
                enabled: (root.details.folder || "").length > 0
                objectName: "settings.dosi.folder"
                Accessible.id: objectName
                onClicked: Models.DesktopFiles.openPath(root.details.folder)
            }
        }
        ListRow {
            title: qsTr("Chat")
            subtitle: root.details.conversation || ""
            iconName: "chat"
            tint: "#d97757"

            KButton {
                id: renew
                compact: true
                text: qsTr("New chat")
                enabled: !Models.Dosi.empty
                objectName: "settings.dosi.renew"
                Accessible.id: objectName
                onClicked: confirm.open()

                KPopover {
                    id: confirm

                    x: renew.width - width
                    y: renew.height + 8
                    transformOrigin: Popup.TopRight
                    width: 280
                    padding: 14

                    contentItem: ColumnLayout {
                        spacing: 10

                        PlainLabel {
                            text: qsTr("Start a new chat?")
                            font.pixelSize: KodosiTheme.fontHeadline
                            font.weight: Font.DemiBold
                        }
                        PlainLabel {
                            Layout.fillWidth: true
                            text: qsTr("Dosi forgets this chat, here and on your other computers.")
                            color: KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            wrapMode: Text.WordWrap
                        }
                        RowLayout {
                            spacing: 6

                            Item { Layout.fillWidth: true }
                            KButton { compact: true; text: qsTr("Cancel"); onClicked: confirm.close() }
                            KButton {
                                compact: true
                                variant: KButton.Danger
                                text: qsTr("Start new chat")
                                onClicked: {
                                    confirm.close()
                                    Models.Dosi.renew()
                                }
                            }
                        }
                    }
                }
            }
        }
        ListRow {
            title: qsTr("Full access")
            subtitle: qsTr("Dosi uses every tool of its agent and does not ask first.")
            iconName: "check"
            tint: "#2f7d54"
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        visible: Models.DesktopState.dosiOn && text.length > 0
        text: root.instructions.message || ""
        color: KodosiTheme.danger
        wrapMode: Text.WordWrap
    }

    component Picker: KButton {
        id: picker

        property var values: []
        property string current: ""

        signal picked(string value)

        compact: true
        iconName: "chevron-down"
        iconTrailing: true
        text: (values.find(entry => entry.value === current) || values[0] || { label: "" }).label
        Accessible.id: objectName
        onClicked: menu.popup(picker, 0, picker.height + 4)

        KMenu {
            id: menu

            Instantiator {
                model: picker.values
                delegate: KMenuItem {
                    required property var modelData
                    text: modelData.label
                    iconName: modelData.value === picker.current ? "check" : ""
                    onTriggered: picker.picked(modelData.value)
                }
                onObjectAdded: (index, object) => menu.insertItem(index, object)
                onObjectRemoved: (index, object) => menu.removeItem(object)
            }
        }
    }
}
