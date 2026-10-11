pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property bool hasCode: Models.Account.userCode.length > 0
    readonly property var ownedRooms: Models.Missions.missions
        .filter(room => room.ownerUserId === Models.Account.userId)
        .map(room => room.name)

    spacing: 18

    Connections {
        function onDeletionChanged() {
            if (Models.Account.deleting)
                Models.DesktopFiles.openWebUrl(Models.Account.deletionUri)
        }

        target: Models.Account
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: 10
        visible: root.hasCode
        spacing: 16

        PlainLabel {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Enter this code in your browser")
            color: KodosiTheme.inkMuted
        }
        Row {
            Layout.alignment: Qt.AlignHCenter
            spacing: 6
            Accessible.role: Accessible.StaticText
            Accessible.name: Models.Account.userCode
            objectName: "panel.settings.account.code"
            Accessible.id: objectName

            Repeater {
                model: Models.Account.userCode.split("")

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
                variant: KButton.Primary
                iconName: "arrow-right"
                iconTrailing: true
                objectName: "panel.settingsView.open-sign-in-page"
                text: qsTr("Open the sign-in page")

                onClicked: Models.DesktopFiles.openWebUrl(Models.Account.verificationUri)
            }
            KButton {
                variant: KButton.Ghost
                text: qsTr("Cancel")
                objectName: "panel.settings.account.cancel"
                Accessible.id: objectName
                onClicked: Models.Account.cancelLogin()
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 8
            CursorBlock { Layout.preferredWidth: 6; Layout.preferredHeight: 12; blinks: true }
            ShimmerText { text: qsTr("Waiting for you"); active: true; color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: !root.hasCode

        ListRow {
            title: Models.Account.signedIn ? Identity.selfName : qsTr("Not signed in")
            subtitle: Models.Account.signedIn ? qsTr("Signed in") : qsTr("Your terminals work without an account.")
            leading: Item {
                implicitWidth: 32
                implicitHeight: 32
                PersonAvatar { visible: Models.Account.signedIn; name: Identity.selfName; isSelf: true; size: 32 }
                IconTile { visible: !Models.Account.signedIn; iconName: "people"; tint: "#54433a"; size: 32 }
            }

            KButton {
                Accessible.id: objectName
                variant: Models.Account.signedIn ? KButton.Secondary : KButton.Primary
                enabled: !Models.Account.signingIn
                working: Models.Account.signingIn
                objectName: "panel.settings.account"
                text: Models.Account.signedIn ? qsTr("Sign out…") : Models.Account.signingIn ? qsTr("Signing in…") : qsTr("Sign in")

                onClicked: Models.Account.signedIn ? signOutConfirmation.open() : Models.Account.login()
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: !root.hasCode

        ListRow {
            visible: Models.Account.signedIn
            iconName: "close"
            tint: "#b23a32"
            title: Models.Account.deleting ? qsTr("Confirm the deletion in your browser") : qsTr("Delete your account")
            subtitle: Models.Account.deleting
                ? qsTr("Kodosi waits for your confirmation on the page.")
                : qsTr("Removes your account, devices and friends.")

            KButton {
                Accessible.id: objectName
                compact: true
                visible: !Models.Account.deleting
                variant: KButton.Danger
                objectName: "panel.settingsView.delete-account"
                text: qsTr("Delete…")

                onClicked: deleteConfirmation.open()
            }
            KButton {
                Accessible.id: objectName
                compact: true
                visible: Models.Account.deleting
                variant: KButton.Secondary
                objectName: "panel.settingsView.deletion-page"
                text: qsTr("Open the page")

                onClicked: Models.DesktopFiles.openWebUrl(Models.Account.deletionUri)
            }
            KButton {
                Accessible.id: objectName
                compact: true
                visible: Models.Account.deleting
                variant: KButton.Ghost
                objectName: "panel.settingsView.deletion-cancel"
                text: qsTr("Cancel")

                onClicked: Models.Account.cancelDeletion()
            }
        }
        ListRow {
            iconName: "minus"
            tint: "#54433a"
            title: qsTr("Quit Kodosi")
            subtitle: qsTr("Closing the window keeps your terminals running. Quit stops them.")

            KButton {
                Accessible.id: objectName
                compact: true
                objectName: "panel.settingsView.quit-kodosi"
                text: qsTr("Quit…")

                onClicked: quitConfirmation.open()
            }
        }
    }
    KDialog {
        id: signOutConfirmation

        objectName: "panel.settings.account.confirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Sign out?")

        onAccepted: Models.Account.logout()
        onOpened: standardButton(Dialog.Ok).text = qsTr("Sign out")

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: qsTr("Sharing stops. The terminals on this computer keep running.")
            width: 340
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: deleteConfirmation

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Delete your Kodosi account?")
        onOpened: standardButton(Dialog.Ok).text = qsTr("Delete Account")

        onAccepted: Models.Account.deleteAccount()

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: [qsTr("Your devices, friends and sharing go away. What you wrote in other rooms stays there.")]
                .concat(root.ownedRooms.length > 0
                    ? [qsTr("These rooms close for everyone in them: %1.").arg(root.ownedRooms.join(", "))]
                    : [])
                .concat([qsTr("The terminals on this computer keep running. This cannot be undone.")])
                .join("\n\n")
            width: 380
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: quitConfirmation

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Quit Kodosi?")
        onOpened: standardButton(Dialog.Ok).text = qsTr("Quit")

        onAccepted: Qt.quit()

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            text: qsTr("The terminals on this computer stop.")
            wrapMode: Text.WordWrap
        }
    }
}
