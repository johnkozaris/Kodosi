pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property bool inBrowser: Models.Account.signInPage.length > 0
    readonly property var ownedProjects: Models.Projects.projects
        .filter(project => project.ownerUserId === Models.Account.userId)
        .map(project => project.name)

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
            iconName: "globe"
            tint: "#3f7fbf"
            title: qsTr("Language")
            subtitle: Models.DesktopSettings.nextLanguage !== Models.DesktopSettings.currentLanguage
                ? qsTr("Quit Kodosi and open it again to show %1.").arg(Models.DesktopSettings.languageName(Models.DesktopSettings.nextLanguage))
                : qsTr("Kodosi shows the language of your system, or the one that you choose here.")

            KPicker {
                objectName: "settings.language"
                values: [{ value: "", label: qsTr("Same as the system") }].concat(Models.DesktopSettings.languages
                    .map(entry => ({ value: entry.tag, label: entry.name })))
                current: Models.DesktopSettings.language
                onPicked: value => Models.DesktopSettings.language = value
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
            iconName: "sun"
            tint: "#d98a5f"
            title: qsTr("Stay awake")
            wholeSubtitle: true
            subtitle: Models.Devices.keepAwake === "always"
                ? qsTr("This computer does not go to sleep by itself while Kodosi runs. On battery, it stays awake only while agents work.")
                : Models.Devices.keepAwake === "never"
                ? qsTr("This computer can go to sleep while an agent works. The agent then pauses until the computer wakes.")
                : qsTr("This computer does not go to sleep by itself while an agent works, or for a few minutes after. An agent that waits for you does not keep it awake.")

            SegmentedPill {
                compact: true
                identifier: "settings.keepAwake"
                options: [
                    { value: "agents", label: qsTr("While agents work") },
                    { value: "always", label: qsTr("Always") },
                    { value: "never", label: qsTr("Never") }
                ]
                currentValue: Models.Devices.keepAwake
                onActivated: value => Models.Devices.setKeepAwake(value)
            }
        }
        ListRow {
            iconName: "bell"
            tint: "#cf6a62"
            title: qsTr("Notifications")
            subtitle: qsTr("When a terminal waits for you, finishes or fails")

            KSwitch {
                Accessible.id: objectName
                Accessible.name: qsTr("Notifications")
                checked: Models.DesktopSettings.notifies
                objectName: "settings.notifies"

                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => Models.DesktopSettings.notifies);
                    Models.DesktopSettings.notifies = wanted;
                }
            }
        }
        ListRow {
            iconName: "canvas"
            tint: "#d98a5f"
            title: qsTr("Open the canvas by itself")
            subtitle: Models.DesktopSettings.opensCanvas
                ? qsTr("The canvas opens when the agent shows something on it. A change of a file or the browser on it opens it too, unless you closed it.")
                : qsTr("The canvas opens only when you open it. Its keys show what is new.")

            KSwitch {
                Accessible.id: objectName
                Accessible.name: qsTr("Open the canvas by itself")
                checked: Models.DesktopSettings.opensCanvas
                objectName: "settings.opensCanvas"

                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => Models.DesktopSettings.opensCanvas);
                    Models.DesktopSettings.opensCanvas = wanted;
                }
            }
        }
        ListRow {
            iconName: "minus"
            tint: "#54433a"
            title: qsTr("Quit Kodosi")
            subtitle: Models.ApplicationLifecycle.hostForeground
                ? qsTr("Your terminals keep running in the kodosi host command.")
                : qsTr("Terminals stop. They start again when Kodosi opens.")

            KButton {
                Accessible.id: objectName
                compact: true
                objectName: "panel.settingsView.quit-kodosi"
                text: qsTr("Quit")

                onClicked: quitConfirmation.request(false)
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true
        visible: !root.inBrowser

        Repeater {
            model: [
                { page: "privacy", title: qsTr("Privacy policy"), iconName: "lock", tint: "#5b82c9" },
                { page: "terms", title: qsTr("Terms of use"), iconName: "document", tint: "#6b6560" },
                { page: "support", title: qsTr("Support"), iconName: "question", tint: "#5fa57e" }
            ]

            delegate: ListRow {
                id: link

                required property var modelData

                iconName: link.modelData.iconName
                tint: link.modelData.tint
                title: link.modelData.title

                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: link.modelData.title
                    glyph: "arrow-right"
                    objectName: "panel.settingsView." + link.modelData.page

                    onClicked: Qt.openUrlExternally("https://kodosi.com/" + link.modelData.page)
                }
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
            text: [qsTr("Kodosi deletes your devices, friends and sharing. What you wrote in other projects stays there.")]
                .concat(root.ownedProjects.length > 0
                    ? [qsTr("These projects close for everyone in them: %1.").arg(root.ownedProjects.join(", "))]
                    : [])
                .concat([qsTr("The terminals on this computer keep running. You cannot undo this.")])
                .join("\n\n")
            width: 380
            wrapMode: Text.WordWrap
        }
    }
    QuitDialog {
        id: quitConfirmation
    }
}
