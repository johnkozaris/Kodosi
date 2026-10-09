pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property bool waiting: Models.Devices.approvalCode.length > 0

    spacing: 18

    EmptyState {
        Layout.fillWidth: true
        Layout.topMargin: 30
        visible: !Models.Account.signedIn
        title: qsTr("Sign in to see your devices")
        art: IconTile { iconName: "laptop"; tint: "#4f9bb0"; size: 48 }

        KButton {
            variant: KButton.Primary
            text: qsTr("Sign in")
            onClicked: Models.Account.login()
        }
    }
    ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: 10
        spacing: 16
        visible: Models.Account.signedIn && !Models.Devices.localDeviceEnrolled

        Row {
            Layout.alignment: Qt.AlignHCenter
            spacing: 14

            IconTile { iconName: "laptop"; tint: "#54433a"; size: 44 }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 5
                Repeater {
                    model: 3
                    Rectangle {
                        id: dot
                        required property int index
                        anchors.verticalCenter: parent.verticalCenter
                        width: 5
                        height: 5
                        radius: 2.5
                        color: KodosiTheme.accent
                        opacity: 0.3

                        SequentialAnimation on opacity {
                            running: root.waiting && !KodosiTheme.reduceMotion
                            loops: Animation.Infinite
                            PauseAnimation { duration: dot.index * 180 }
                            NumberAnimation { to: 1; duration: 260 }
                            NumberAnimation { to: 0.3; duration: 260 }
                            PauseAnimation { duration: (2 - dot.index) * 180 + 300 }
                        }
                    }
                }
            }
            IconTile { iconName: "laptop"; tint: KodosiTheme.accent; size: 44 }
        }
        EmptyState {
            Layout.fillWidth: true
            title: root.waiting ? qsTr("Type this code on a device that you use") : qsTr("Approve this computer")
            message: root.waiting ? qsTr("Open Settings, then Devices, on that device.") : qsTr("A device that you use must approve this computer one time.")
        }
        Row {
            Layout.alignment: Qt.AlignHCenter
            visible: root.waiting
            spacing: 6
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Device approval code") + " " + Models.Devices.approvalCode
            objectName: "panel.settings.enrollment.reason"
            Accessible.id: objectName

            Repeater {
                model: Models.Devices.approvalCode.split("")

                Item {
                    id: key

                    required property string modelData
                    readonly property bool separator: modelData === "-" || modelData === " "

                    width: separator ? 12 : 40
                    height: 52

                    Raised { anchors.fill: parent; visible: !key.separator; radius: KodosiTheme.radiusMd }
                    Text {
                        anchors.centerIn: parent
                        text: key.modelData
                        color: key.separator ? KodosiTheme.inkFaint : KodosiTheme.ink
                        font.family: "monospace"
                        font.pixelSize: 24
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 8

            KButton {
                Accessible.id: objectName
                variant: root.waiting ? KButton.Ghost : KButton.Primary
                objectName: "panel.settings.enrollment"
                text: root.waiting ? qsTr("Cancel") : qsTr("Get a code")

                onClicked: root.waiting ? Models.Devices.cancelApproval() : Models.Devices.requestApproval()
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.settings.start-fresh"
                text: qsTr("Start fresh…")
                variant: KButton.Ghost
                visible: !root.waiting

                onClicked: startFresh.open()
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: Models.Devices.notice.length > 0
            text: Models.Devices.notice
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.Account.signedIn && Models.Devices.devices.length > 0
        title: qsTr("Your devices")

        Repeater {
            model: Models.Devices.devices

            delegate: ListRow {
                id: deviceRow

                required property var modelData
                readonly property bool self: modelData.deviceId === Models.Devices.selfDeviceId

                iconName: "laptop"
                tint: self ? KodosiTheme.accent : "#4f9bb0"
                title: deviceRow.modelData.label

                Tag { visible: deviceRow.self; text: qsTr("This computer"); tone: Tag.Accent }
                KButton {
                    Accessible.id: objectName
                    compact: true
                    objectName: "panel.settingsView.remove" + "." + deviceRow.modelData.deviceId
                    text: qsTr("Remove…")
                    visible: Models.Devices.localDeviceEnrolled && !deviceRow.self
                    variant: KButton.Ghost

                    onClicked: {
                        revoke.deviceId = deviceRow.modelData.deviceId;
                        revoke.label = deviceRow.modelData.label;
                        revoke.open();
                    }
                }
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
        title: qsTr("Approve another device")
        footer: qsTr("Type the code that the other device shows.")

        Repeater {
            model: Models.Devices.requests

            delegate: ListRow {
                required property var modelData

                iconName: "laptop"
                tint: "#d9a441"
                title: qsTr("%1 wants to use your account").arg(modelData.deviceLabel)
            }
        }
        ListRow {
            iconName: "lock"
            tint: "#7dbb99"
            title: qsTr("Code")

            KTextField {
                id: code

                Accessible.id: objectName
                Accessible.name: qsTr("Device approval code")
                Layout.preferredWidth: 180
                Layout.preferredHeight: 30
                font.family: "monospace"
                objectName: "panel.settingsView.code"
                placeholderText: qsTr("Code")
                onAccepted: approve.clicked()
            }
            KButton {
                id: approve
                Accessible.id: objectName
                compact: true
                variant: KButton.Primary
                enabled: code.text.trim().length > 0
                objectName: "panel.settingsView.approve-device"
                text: qsTr("Approve")

                onClicked: {
                    Models.Devices.approve(code.text.trim());
                    code.clear();
                }
            }
        }
    }
    KDialog {
        id: startFresh

        destructive: true
        objectName: "panel.settings.start-fresh.confirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Start fresh on this computer?")

        onAccepted: Models.Devices.startFresh()
        onOpened: standardButton(Dialog.Ok).text = qsTr("Start fresh")

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: qsTr("Your other devices lose access until you approve them again from this computer. Friends must trust you again. Shared terminals stay shared.")
            width: 380
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: revoke

        property string deviceId: ""
        property string label: ""

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Remove %1?").arg(label)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Remove")

        onAccepted: Models.Devices.revoke(deviceId)

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            text: qsTr("It loses your account and the terminals shared with you.")
            wrapMode: Text.WordWrap
        }
    }
}
