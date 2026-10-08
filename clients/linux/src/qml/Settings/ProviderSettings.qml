pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    spacing: 12

    RowLayout {
        KComboBox {
            id: provider

            Accessible.id: objectName
            Accessible.name: qsTr("Provider")
            model: ["Claude", "Copilot"]
            objectName: "panel.settingsView.provider"
        }
        KButton {
            Accessible.id: objectName
            enabled: !Models.ProviderFiles.busy
            objectName: "panel.settingsView.show-configuration-files"
            text: qsTr("Show files")

            onClicked: Models.ProviderFiles.inspect(
                provider.currentIndex === 0
                    ? Models.ProviderFiles.Claude
                    : Models.ProviderFiles.Copilot,
                Models.DesktopSettings.effectiveWorkingDirectory)
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        color: KodosiTheme.textSecondary
        text: Models.ProviderFiles.installation.executable || Models.ProviderFiles.installation.message || ""
        wrapMode: Text.WordWrap
    }
    Repeater {
        model: Models.ProviderFiles.installation.files || []

        delegate: RowLayout {
            id: fileRow

            required property var modelData

            Layout.fillWidth: true

            ColumnLayout {
                Layout.fillWidth: true

                PlainLabel {
                    color: KodosiTheme.textPrimary
                    text: fileRow.modelData.label
                }
                PlainLabel {
                    Layout.fillWidth: true
                    color: KodosiTheme.textSecondary
                    elide: Text.ElideMiddle
                    text: fileRow.modelData.path
                }
            }
            KButton {
                Accessible.id: objectName
                enabled: fileRow.modelData.exists
                objectName: "panel.settingsView.open" + "." + fileRow.modelData.label
                text: qsTr("Open")

                onClicked: Models.DesktopFiles.openPath(fileRow.modelData.path)
            }
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        color: KodosiTheme.danger
        text: Models.ProviderFiles.error
        visible: Models.ProviderFiles.error.length > 0
        wrapMode: Text.WordWrap
    }
}
