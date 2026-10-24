pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property bool inBrowser: Models.Account.signInPage.length > 0
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
        visible: root.inBrowser
        spacing: 16

        PlainLabel {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Continue in your browser")
            color: KodosiTheme.inkMuted
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
                text: qsTr("Open the browser again")

                onClicked: Models.DesktopFiles.openWebUrl(Models.Account.signInPage)
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
        visible: !root.inBrowser
        footer: {
            const folder = Models.Account.credentialFolder;
            switch (Models.Account.credentialStore) {
            case "keychain":
                return qsTr("Kodosi keeps your sign-in in the keychain (Secret Service).");
            case "privateFile":
                return qsTr("Kodosi keeps your sign-in in private files in %1.").arg(folder);
            case "protectedFile":
                return qsTr("Kodosi keeps your sign-in in files that Windows protects for you, in %1.").arg(folder);
            default:
                return "";
            }
        }

        ListRow {
            title: Models.Account.signedIn ? Identity.selfName
                : Models.Account.restoring.length > 0 ? qsTr("Signing in again") : qsTr("Not signed in")
            subtitle: {
                if (Models.Account.signedIn)
                    return qsTr("Signed in");
                if (Models.Account.restoring === "offline")
                    return qsTr("Kodosi signs you in when it reaches the server.");
                if (Models.Account.restoring === "updateRequired")
                    return qsTr("Update Kodosi to sign in.");
                if (Models.Account.restoring === "unavailable")
                    return qsTr("The Kodosi server does not answer. Kodosi tries again.");
                if (Models.Account.restoring === "unreadable")
                    return Models.Account.credentialStore === "keychain" ? qsTr("Unlock the keychain. Kodosi tries again.")
                        : qsTr("Kodosi cannot read your sign-in in %1.").arg(Models.Account.credentialFolder);
                return qsTr("Your terminals work without an account.");
            }
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
                text: Models.Account.signedIn ? qsTr("Sign out…") : Models.Account.signingIn ? qsTr("Signing in…")
                    : Models.Account.restoring.length > 0 ? qsTr("Try again") : qsTr("Sign in")

                onClicked: Models.Account.signedIn ? signOutConfirmation.open() : Models.Account.login()
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: !root.inBrowser

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
            iconName: "window-restore"
            tint: "#4f9bb0"
            title: qsTr("Keep running when the window closes")
            subtitle: !Models.DesktopSettings.keepRunning ? qsTr("Closing the window quits Kodosi.")
                : Models.Tray.available ? qsTr("Your terminals keep running, and Kodosi stays in the system tray.")
                : qsTr("Your terminals keep running. Open Kodosi again to see them.")

            KSwitch {
                Accessible.id: objectName
                Accessible.name: qsTr("Keep running when the window closes")
                checked: Models.DesktopSettings.keepRunning
                objectName: "settings.keepRunning"

                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => Models.DesktopSettings.keepRunning);
                    Models.DesktopSettings.keepRunning = wanted;
                }
            }
        }
        ListRow {
            iconName: "minus"
            tint: "#54433a"
            title: qsTr("Quit Kodosi")
            subtitle: Models.ApplicationLifecycle.hostForeground
                ? qsTr("Your terminals keep running in the kodosi host command.")
                : qsTr("Quit stops the terminals on this computer.")

            KButton {
                Accessible.id: objectName
                compact: true
                objectName: "panel.settingsView.quit-kodosi"
                text: qsTr("Quit")

                onClicked: quitConfirmation.request(false)
            }
        }
    }
    SignOutDialog {
        id: signOutConfirmation
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
    QuitDialog {
        id: quitConfirmation
    }
}
