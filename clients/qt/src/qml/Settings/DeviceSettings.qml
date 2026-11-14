pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property bool waiting: Models.Devices.approvalCode.length > 0
    readonly property var reset: Models.Devices.pendingReset
    readonly property bool resetWaits: reset.thisDevice === true
    readonly property bool resetAsked: reset.resetId !== undefined && !resetWaits
    property bool recovering: false
    property bool copied: false

    function focusCode() {
        code.forceActiveFocus();
    }

    function readyAt() {
        const ready = new Date(root.reset.readyAtMs);
        return Qt.locale().dayName(ready.getDay(), Locale.LongFormat) + " " + ready.toLocaleTimeString(Qt.locale(), Locale.ShortFormat);
    }

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

                        SequentialAnimation {
                            running: root.waiting && !KodosiTheme.reduceMotion && KodosiTheme.shown
                            loops: Animation.Infinite
                            PauseAnimation { duration: dot.index * 180 }
                            NumberAnimation { target: dot; property: "opacity"; from: 0.3; to: 1; duration: 260 }
                            NumberAnimation { target: dot; property: "opacity"; from: 1; to: 0.3; duration: 260 }
                            PauseAnimation { duration: (2 - dot.index) * 180 + 300 }
                        }
                    }
                }
            }
            IconTile { iconName: "laptop"; tint: KodosiTheme.accent; size: 44 }
        }
        EmptyState {
            Layout.fillWidth: true
            title: root.waiting ? qsTr("Scan or type this code on a device that you use") : qsTr("Approve this computer")
            message: root.waiting ? qsTr("On that device, open Settings and choose Add a device.")
                : root.resetWaits ? qsTr("This computer can start fresh after %1. Until then, your other devices can stop it. If you still have one of them, approve this computer with it instead, and keep your friends and projects.").arg(root.readyAt())
                : qsTr("A device that you use must approve this computer one time.")
        }
        ScanCode {
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 200
            Layout.preferredHeight: 200
            visible: root.waiting && Models.Devices.approvalQr.length > 0
            rows: Models.Devices.approvalQr
            Accessible.name: qsTr("QR code of this code")
            objectName: "panel.settings.enrollment.qr"
            Accessible.id: objectName
        }
        CodeKeys {
            Layout.alignment: Qt.AlignHCenter
            visible: root.waiting
            code: Models.Devices.approvalCode
            Accessible.name: qsTr("Device approval code") + " " + Models.Devices.approvalCode
            objectName: "panel.settings.enrollment.reason"
            Accessible.id: objectName
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 8

            KButton {
                Accessible.id: objectName
                variant: root.waiting ? KButton.Ghost : KButton.Primary
                objectName: "panel.settings.enrollment"
                text: root.waiting ? qsTr("Cancel") : qsTr("Get a code")
                visible: !root.resetWaits

                onClicked: root.waiting ? Models.Devices.cancelApproval() : Models.Devices.requestApproval()
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.settings.use-recovery-key"
                text: qsTr("Use my recovery key")
                visible: !root.waiting && !root.recovering && !root.resetWaits

                onClicked: root.recovering = true
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.settings.start-fresh"
                text: root.resetWaits ? qsTr("Start fresh") : qsTr("Start fresh…")
                variant: root.resetWaits ? KButton.Primary : KButton.Ghost
                visible: !root.waiting

                onClicked: root.resetWaits ? Models.Devices.startFresh() : startFresh.open()
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.settings.stop-reset"
                text: qsTr("Do not start fresh")
                variant: KButton.Ghost
                visible: root.resetWaits

                onClicked: Models.Devices.stopReset()
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            visible: root.recovering && !root.waiting
            spacing: 8

            KTextField {
                id: recoveryKeyField

                Accessible.id: objectName
                Accessible.name: qsTr("Recovery key")
                Layout.preferredWidth: 380
                Layout.preferredHeight: 30
                font.family: "monospace"
                objectName: "panel.settings.recovery-key"
                placeholderText: qsTr("Recovery key")
                onAccepted: recover.clicked()
            }
            KButton {
                id: recover

                Accessible.id: objectName
                compact: true
                variant: KButton.Primary
                enabled: recoveryKeyField.text.trim().length > 0
                objectName: "panel.settings.recover"
                text: qsTr("Approve")

                onClicked: Models.Devices.useRecoveryKey(recoveryKeyField.text.trim())
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
        visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled && root.resetAsked
        title: qsTr("%1 asked to start fresh").arg(root.reset.deviceLabel ?? "")
        footer: qsTr("If nobody stops it, it can start fresh after %1, and all your devices lose access. Stop it unless you asked for it.").arg(root.resetAsked ? root.readyAt() : "")

        ListRow {
            iconName: "warning"
            tint: KodosiTheme.danger
            title: root.reset.deviceLabel ?? ""

            KButton {
                Accessible.id: objectName
                compact: true
                variant: KButton.Primary
                objectName: "panel.settingsView.reset.stop"
                text: qsTr("Stop it")

                onClicked: Models.Devices.stopReset()
            }
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
                readonly property bool recoveryKey: modelData.recoveryKey === true

                iconName: recoveryKey ? "lock" : "laptop"
                tint: self ? KodosiTheme.accent : "#4f9bb0"
                title: recoveryKey ? qsTr("Recovery key") : deviceRow.modelData.label
                subtitle: recoveryKey ? qsTr("Approves a new device when you have no other") : ""

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
                        revoke.label = deviceRow.title;
                        revoke.removesRecoveryKey = deviceRow.recoveryKey;
                        revoke.open();
                    }
                }
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
        title: qsTr("Add a device")
        footer: qsTr("A new phone scans a code from this computer. For another device, type the code that it shows.")

        Repeater {
            model: Models.Devices.requests

            delegate: ListRow {
                required property var modelData

                iconName: "laptop"
                tint: "#4f9bb0"
                title: modelData.deviceLabel
                subtitle: qsTr("Waiting for approval")

                StatusSign { session: ({ sign: "hand", asks: true, signLabel: qsTr("Needs your approval") }) }
            }
        }
        ListRow {
            iconName: "phone"
            tint: KodosiTheme.accent
            title: qsTr("A new phone scans a code")

            KButton {
                Accessible.id: objectName
                compact: true
                objectName: "panel.settingsView.offer-device"
                text: qsTr("Show a code")

                onClicked: {
                    Models.Devices.offer();
                    offer.open();
                }
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
    ListGroup {
        Layout.fillWidth: true
        visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
        title: qsTr("If you lose all your devices")
        footer: qsTr("A recovery key approves a new device when you have no other device.")

        ListRow {
            iconName: "lock"
            tint: "#d9a441"
            title: qsTr("Recovery key")

            KButton {
                Accessible.id: objectName
                compact: true
                objectName: "panel.settingsView.recovery-key"
                text: Models.Devices.hasRecoveryKey ? qsTr("Make a new key…") : qsTr("Make a recovery key")

                onClicked: Models.Devices.hasRecoveryKey ? replaceRecoveryKey.open() : Models.Devices.makeRecoveryKey()
            }
        }
    }
    Connections {
        target: Models.Devices

        function onChanged() {
            if (Models.Devices.newRecoveryKey.length > 0 && !newRecoveryKey.opened) {
                root.copied = false;
                newRecoveryKey.open();
            }
            if (Models.Devices.localDeviceEnrolled) {
                root.recovering = false;
                recoveryKeyField.clear();
            }
            if (Models.Devices.checkedLabel.length > 0 && !approveChecked.opened)
                approveChecked.open();
            if (Models.Devices.offerState.length === 0 && offer.opened)
                offer.close();
        }
    }
    KDialog {
        id: offer

        readonly property string phase: Models.Devices.offerState
        readonly property bool showing: phase === "requesting" || phase === "shown"

        function nameClose() {
            const close = standardButton(Dialog.Close);
            if (close)
                close.text = phase === "approved" ? qsTr("Done")
                    : phase === "ended" || phase === "failed" ? qsTr("Close") : qsTr("Cancel");
        }

        objectName: "panel.settings.offer"
        standardButtons: Dialog.Close
        title: phase === "approved" ? "" : qsTr("Add a phone")

        onClosed: Models.Devices.endOffer()
        onPhaseChanged: nameClose()
        onOpened: nameClose()

        ColumnLayout {
            spacing: 16

            PlainLabel {
                Layout.fillWidth: true
                Layout.preferredWidth: 340
                visible: offer.showing
                color: KodosiTheme.inkMuted
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("On the new phone, open Kodosi and sign in. Then choose Scan the code on your computer.")
                wrapMode: Text.WordWrap
            }
            ScanCode {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: 240
                Layout.preferredHeight: 240
                visible: offer.showing
                rows: Models.Devices.offerQr
                Accessible.name: qsTr("QR code for the new phone")
                objectName: "panel.settings.offer.qr"
                Accessible.id: objectName
            }
            PlainLabel {
                Layout.fillWidth: true
                Layout.preferredWidth: 340
                visible: offer.showing
                color: KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontFootnote
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("Waiting for the phone")
            }
            IconTile {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 12
                visible: offer.phase === "approved"
                iconName: "check"
                tint: KodosiTheme.ready
                size: 48
            }
            PlainLabel {
                Accessible.id: objectName
                Layout.fillWidth: true
                Layout.preferredWidth: 340
                visible: offer.phase === "approved" || offer.phase === "ended"
                font.pixelSize: KodosiTheme.fontHeadline + 2
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
                objectName: "panel.settings.offer.result"
                text: offer.phase === "ended" ? qsTr("This code no longer works.")
                    : Models.Devices.offerDetail.length > 0 ? qsTr("%1 is approved.").arg(Models.Devices.offerDetail)
                    : qsTr("The device is approved.")
                wrapMode: Text.WordWrap
            }
            PlainLabel {
                Layout.fillWidth: true
                Layout.preferredWidth: 340
                visible: offer.phase === "approved"
                color: KodosiTheme.inkMuted
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("It can use your terminals and projects now.")
                wrapMode: Text.WordWrap
            }
            PlainLabel {
                Accessible.id: objectName
                Layout.fillWidth: true
                Layout.preferredWidth: 340
                visible: offer.phase === "failed"
                color: KodosiTheme.danger
                horizontalAlignment: Text.AlignHCenter
                objectName: "panel.settings.offer.error"
                text: Models.Devices.offerDetail
                wrapMode: Text.WordWrap
            }
            KButton {
                Accessible.id: objectName
                Layout.alignment: Qt.AlignHCenter
                visible: offer.phase === "ended" || offer.phase === "failed"
                variant: KButton.Primary
                objectName: "panel.settings.offer.again"
                text: qsTr("Show a new code")

                onClicked: Models.Devices.offer()
            }
        }
    }
    KDialog {
        id: approveChecked

        objectName: "panel.settings.approve.confirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Approve %1?").arg(Models.Devices.checkedLabel)

        onAccepted: Models.Devices.confirmApproval()
        onRejected: Models.Devices.dropApproval()
        onOpened: standardButton(Dialog.Ok).text = qsTr("Approve")

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: qsTr("It can then use all your terminals, projects and friends. Approve it only if you hold that device.")
            width: 380
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: replaceRecoveryKey

        objectName: "panel.settings.recovery-key.replace"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Make a new recovery key?")

        onAccepted: Models.Devices.makeRecoveryKey()
        onOpened: standardButton(Dialog.Ok).text = qsTr("Make a new key")

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: qsTr("The recovery key that you have now stops working.")
            width: 340
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: newRecoveryKey

        closePolicy: Popup.NoAutoClose
        objectName: "panel.settings.recovery-key.new"
        standardButtons: Dialog.Ok
        title: qsTr("Your recovery key")

        onClosed: Models.Devices.forgetNewRecoveryKey()
        onOpened: standardButton(Dialog.Ok).text = qsTr("I saved it")

        ColumnLayout {
            spacing: 14
            width: 400

            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.inkMuted
                text: qsTr("Keep it in a safe place, such as a password manager. It approves a new device when you have no other device. Kodosi does not show it again.")
                wrapMode: Text.WordWrap
            }
            KReadOnlyText {
                Accessible.id: objectName
                Accessible.name: qsTr("Recovery key")
                Layout.fillWidth: true
                font.family: "monospace"
                font.pixelSize: 16
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
                objectName: "panel.settings.recovery-key.text"
                text: Models.Devices.newRecoveryKey
            }
            KButton {
                Accessible.id: objectName
                Layout.alignment: Qt.AlignHCenter
                iconName: root.copied ? "check" : "copy"
                objectName: "panel.settings.recovery-key.copy"
                text: root.copied ? qsTr("Copied") : qsTr("Copy")

                onClicked: {
                    Models.Devices.copyNewRecoveryKey();
                    root.copied = true;
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
            text: qsTr("Your other devices lose access until you approve them again from this computer. Friends must trust you again, and you make your projects again. When your account has devices, this computer waits a day. If you have another device or a recovery key, cancel and use it: you keep your friends and projects.")
            width: 380
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: revoke

        property string deviceId: ""
        property string label: ""
        property bool removesRecoveryKey: false

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Remove %1?").arg(label)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Remove")

        onAccepted: Models.Devices.revoke(deviceId)

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            text: revoke.removesRecoveryKey ? qsTr("The recovery key stops working. You can make a new one.")
                : Models.Devices.hasRecoveryKey ? qsTr("It loses your account and the terminals shared with you. If it was lost or stolen, also make a new recovery key.")
                : qsTr("It loses your account and the terminals shared with you.")
            wrapMode: Text.WordWrap
        }
    }
}
