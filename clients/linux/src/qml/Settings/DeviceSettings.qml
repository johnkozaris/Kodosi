pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    spacing: 12

    PlainLabel {
        color: KodosiTheme.textSecondary
        text: qsTr("Sign in to manage devices.")
        visible: !Models.Account.signedIn
    }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8
        visible: Models.Account.signedIn && !Models.Devices.localDeviceEnrolled

        RowLayout {
            Layout.fillWidth: true
            KIcon { name: "computer"; implicitWidth: 44; implicitHeight: 44; color: KodosiTheme.accent }
            KIcon { name: "chevron-right"; implicitWidth: 20; implicitHeight: 20 }
            KIcon { name: "computer"; implicitWidth: 44; implicitHeight: 44 }
            KBusyIndicator { running: Models.Devices.approvalCode.length > 0 }
        }
        KTextField {
            Layout.fillWidth: true
            visible: Models.Devices.approvalCode.length > 0
            text: Models.Devices.approvalCode
            readOnly: true
            selectByMouse: true
            font.pixelSize: 22
            font.letterSpacing: 2
            horizontalAlignment: Text.AlignHCenter
            Accessible.name: qsTr("Device approval code")
            objectName: "panel.settings.enrollment.reason"
        }
        RowLayout {
            spacing: 8

            KButton {
                Accessible.id: objectName
                objectName: "panel.settings.enrollment"
                text: Models.Devices.approvalCode.length ? qsTr("Cancel") : qsTr("Request approval")

                onClicked: Models.Devices.approvalCode.length ? Models.Devices.cancelApproval() : Models.Devices.requestApproval()
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.settings.start-fresh"
                text: qsTr("Start fresh…")
                variant: KButton.Quiet
                visible: Models.Devices.approvalCode.length === 0

                onClicked: startFresh.open()
            }
        }
    }
    Repeater {
        model: Models.Devices.devices

        delegate: RowLayout {
            id: deviceRow

            required property var modelData

            Layout.fillWidth: true

            KIcon { name: "computer"; implicitWidth: 28; implicitHeight: 28; color: KodosiTheme.accent }
            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.textPrimary
                text: deviceRow.modelData.label
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.settingsView.remove" + "." + deviceRow.modelData.deviceId
                text: qsTr("Remove…")
                visible: Models.Devices.localDeviceEnrolled && deviceRow.modelData.deviceId !== Models.Devices.selfDeviceId
                variant: KButton.Quiet

                onClicked: {
                    revoke.deviceId = deviceRow.modelData.deviceId;
                    revoke.label = deviceRow.modelData.label;
                    revoke.open();
                }
            }
        }
    }
    Repeater {
        model: Models.Devices.requests

        delegate: RowLayout {
            id: requestRow

            required property var modelData

            Layout.fillWidth: true

            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.textPrimary
                text: qsTr("%1 wants to use your account").arg(requestRow.modelData.deviceLabel)
                wrapMode: Text.WordWrap
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled

        KTextField {
            id: code

            Accessible.id: objectName
            Accessible.name: qsTr("Device approval code")
            Layout.fillWidth: true
            objectName: "panel.settingsView.code"
            placeholderText: qsTr("Code shown on the other device")
        }
        KButton {
            Accessible.id: objectName
            enabled: code.text.trim().length > 0
            objectName: "panel.settingsView.approve-device"
            text: qsTr("Approve")

            onClicked: {
                Models.Devices.approve(code.text.trim());
                code.clear();
            }
        }
    }
    KDialog {
        id: startFresh

        objectName: "panel.settings.start-fresh.confirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Start fresh on this computer?")

        onAccepted: Models.Devices.startFresh()
        onOpened: standardButton(Dialog.Ok).text = qsTr("Start fresh")

        PlainLabel {
            color: KodosiTheme.textPrimary
            text: qsTr("Every other device loses access to your account and shared terminals until you approve it again from this computer. Friends will be asked to confirm your identity again. Anything already shared stays shared.")
            width: 400
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: revoke

        property string deviceId: ""
        property string label: ""

        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Remove %1?").arg(label)

        onAccepted: Models.Devices.revoke(deviceId)

        PlainLabel {
            color: KodosiTheme.textPrimary
            text: qsTr("This device will lose access to shared terminals.")
        }
    }
}
