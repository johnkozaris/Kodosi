pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property int section: 0

    Accessible.id: objectName
    objectName: "panel.settings"

    KScrollView {
        anchors.fill: parent
        anchors.margins: 24

        ColumnLayout {
            spacing: 14
            width: Math.min(720, parent.width)

            PlainLabel {
                color: KodosiTheme.textPrimary
                font.pixelSize: 22
                text: qsTr("Settings")
            }
            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.danger
                text: Models.Appearance.settingsError
                visible: text.length > 0
                wrapMode: Text.WordWrap
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 4

                Repeater {
                    model: [
                        { key: "general", label: qsTr("General") },
                        { key: "devices", label: qsTr("Devices") },
                        { key: "terminal", label: qsTr("Terminal") },
                        { key: "providers", label: qsTr("Providers") }
                    ]

                    delegate: KButton {
                        required property int index
                        required property var modelData

                        Accessible.id: objectName
                        Accessible.selected: checked
                        Layout.fillWidth: true
                        checkable: true
                        checked: root.section === index
                        objectName: "panel.settings." + modelData.key
                        text: modelData.label
                        variant: KButton.Quiet

                        onClicked: root.section = index
                    }
                }
            }
            GeneralSettings {
                Layout.fillWidth: true
                visible: root.section === 0
            }
            DeviceSettings {
                Layout.fillWidth: true
                visible: root.section === 1
            }
            TerminalSettings {
                Layout.fillWidth: true
                visible: root.section === 2
            }
            ProviderSettings {
                Layout.fillWidth: true
                visible: root.section === 3
            }
        }
    }
}
