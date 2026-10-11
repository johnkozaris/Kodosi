pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    property int provider: 0
    readonly property var installation: Models.ProviderFiles.installation
    readonly property var files: installation.files || []

    function inspect() {
        Models.ProviderFiles.inspect(provider === 0 ? Models.ProviderFiles.Claude : Models.ProviderFiles.Copilot, Models.DesktopSettings.effectiveWorkingDirectory);
    }

    spacing: 18
    onVisibleChanged: {
        if (visible)
            inspect();
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 12

        AgentMark { program: root.provider === 0 ? "claude" : "copilot"; size: 40 }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            PlainLabel { text: root.provider === 0 ? "Claude Code" : "Copilot"; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold }
            ShimmerText {
                Layout.fillWidth: true
                text: Models.ProviderFiles.busy ? qsTr("Looking") : root.installation.executable || root.installation.message || qsTr("Kodosi opens these files. It does not change them.")
                active: Models.ProviderFiles.busy
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                elide: Text.ElideMiddle
            }
        }
        SegmentedPill {
            compact: true
            identifier: "panel.settingsView.provider"
            options: [
                { value: 0, label: "Claude" },
                { value: 1, label: "Copilot" }
            ]
            currentValue: root.provider
            onActivated: value => {
                root.provider = value;
                root.inspect();
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: root.files.length > 0

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
    ListGroup {
        Layout.fillWidth: true
        title: qsTr("Start")
        footer: qsTr("A terminal at its prompt shows these marks. Select a mark to start its command there.")

        Repeater {
            model: Models.DesktopSettings.startCommandIds

            delegate: Item {
                id: startRow

                required property string modelData
                readonly property var command: Models.DesktopSettings.startCommands.find(entry => entry.id === modelData) || ({})

                function save() {
                    Models.DesktopSettings.setStartCommand(modelData, nameField.text, commandField.text)
                }

                width: parent ? parent.width : 0
                implicitHeight: 52

                Rectangle {
                    visible: startRow.Positioner.index > 0
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
                    spacing: 10

                    StartMark { command: startRow.command; size: 22; enabled: false }
                    KTextField {
                        id: nameField
                        Layout.preferredWidth: 170
                        placeholderText: qsTr("Name")
                        objectName: "settings.start." + startRow.modelData + ".name"
                        Accessible.id: objectName
                        Accessible.name: placeholderText
                        Component.onCompleted: text = startRow.command.name || ""
                        onTextEdited: startRow.save()
                    }
                    KTextField {
                        id: commandField
                        Layout.fillWidth: true
                        placeholderText: qsTr("Command")
                        font.family: "monospace"
                        objectName: "settings.start." + startRow.modelData + ".command"
                        Accessible.id: objectName
                        Accessible.name: placeholderText
                        Component.onCompleted: text = startRow.command.command || ""
                        onTextEdited: startRow.save()
                    }
                    KIconButton {
                        glyph: "close"
                        size: 26
                        destructive: true
                        objectName: "settings.start." + startRow.modelData + ".remove"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Remove")
                        onClicked: Models.DesktopSettings.removeStartCommand(startRow.modelData)
                    }
                }
            }
        }
        Item {
            id: addRow

            width: parent ? parent.width : 0
            implicitHeight: 52

            Rectangle {
                visible: addRow.Positioner.index > 0
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                height: 1
                color: KodosiTheme.alpha(KodosiTheme.hairline, 0.6)
            }
            KButton {
                anchors.left: parent.left
                anchors.leftMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Add a command")
                iconName: "plus"
                compact: true
                objectName: "settings.start.add"
                Accessible.id: objectName
                onClicked: Models.DesktopSettings.addStartCommand()
            }
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        color: KodosiTheme.danger
        font.pixelSize: KodosiTheme.fontFootnote
        text: Models.ProviderFiles.error
        visible: Models.ProviderFiles.error.length > 0
        wrapMode: Text.WordWrap
    }
}
