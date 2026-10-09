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
    PlainLabel {
        Layout.fillWidth: true
        color: KodosiTheme.danger
        font.pixelSize: KodosiTheme.fontFootnote
        text: Models.ProviderFiles.error
        visible: Models.ProviderFiles.error.length > 0
        wrapMode: Text.WordWrap
    }
}
