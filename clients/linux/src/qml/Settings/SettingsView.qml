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

    onVisibleChanged: {
        if (!visible)
            return;
        if (!Models.Account.signedIn && Models.Account.userCode.length > 0)
            section = 2;
        else if (Models.Account.signedIn && !Models.Devices.localDeviceEnrolled)
            section = 3;
    }
    Connections {
        function onLoginChanged() {
            if (Models.Account.userCode.length > 0 && !Models.Account.signedIn)
                root.section = 2;
        }

        target: Models.Account
    }
    KScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            x: Math.max(36, (scroll.availableWidth - width) / 2)
            width: Math.min(700, scroll.availableWidth - 72)
            spacing: 22

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 50

                PlainLabel {
                    Layout.fillWidth: true
                    font.pixelSize: KodosiTheme.fontLarge
                    font.weight: Font.DemiBold
                    font.letterSpacing: -0.6
                    text: qsTr("Settings")
                }
                SegmentedPill {
                    identifier: "panel.settings"
                    options: [
                        { value: 0, label: qsTr("Terminal"), icon: "terminal" },
                        { value: 1, label: qsTr("Agents"), icon: "agent" },
                        { value: 2, label: qsTr("Account"), icon: "people" },
                        { value: 3, label: qsTr("Devices"), icon: "laptop" }
                    ]
                    currentValue: root.section
                    onActivated: value => root.section = value
                }
            }
            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.danger
                text: Models.Appearance.settingsError
                visible: text.length > 0
                wrapMode: Text.WordWrap
            }
            TerminalSettings {
                Layout.fillWidth: true
                visible: root.section === 0
            }
            ProviderSettings {
                Layout.fillWidth: true
                visible: root.section === 1
            }
            GeneralSettings {
                Layout.fillWidth: true
                visible: root.section === 2
            }
            DeviceSettings {
                Layout.fillWidth: true
                visible: root.section === 3
            }
            Item { Layout.preferredHeight: 30 }
        }
    }
}
