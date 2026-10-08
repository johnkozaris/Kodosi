pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    Accessible.id: objectName
    objectName: "panel.people"

    KScrollView {
        id: peopleScroll
        anchors.fill: parent
        anchors.margins: 24

        ColumnLayout {
            spacing: 16
            width: Math.min(760, peopleScroll.availableWidth)

            PlainLabel {
                color: KodosiTheme.textPrimary
                font.pixelSize: 22
                text: qsTr("People")
            }
            RowLayout {
                Layout.fillWidth: true

                KTextField {
                    id: handle

                    Accessible.id: objectName
                    Accessible.name: qsTr("Username or invite")
                    Layout.fillWidth: true
                    objectName: "panel.people.username"
                    placeholderText: qsTr("Username or invite")
                }
                KButton {
                    Accessible.id: objectName
                    enabled: Models.Account.signedIn && Models.Devices.localDeviceEnrolled && handle.text.trim().length > 0
                    objectName: "panel.peopleView.add-friend"
                    text: qsTr("Add")

                    onClicked: {
                        Models.People.request(handle.text);
                        handle.clear();
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled

                KButton {
                    Accessible.id: objectName
                    objectName: "panel.peopleView.copy-invite"
                    text: qsTr("Copy my invite")
                    variant: KButton.Quiet

                    onClicked: Models.People.copyInvite()
                }
                PlainLabel {
                    Layout.fillWidth: true
                    color: KodosiTheme.textSecondary
                    text: Models.People.invite.length ? qsTr("Copied") : ""
                    wrapMode: Text.WordWrap
                }
            }
            KButton {
                onClicked: { if (!Models.Account.signedIn) Models.Account.login(); else Models.DesktopState.activeView = Models.DesktopState.Settings; }
                text: Models.Account.signedIn ? qsTr("Connect device") : qsTr("Sign in")
                visible: !Models.Account.signedIn || !Models.Devices.localDeviceEnrolled
            }
            Repeater {
                model: Models.People.incoming

                delegate: RowLayout {
                    id: entry0

                    required property var modelData

                    Layout.fillWidth: true

                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.textPrimary
                        text: entry0.modelData.displayName || entry0.modelData.handle
                    }
                    KButton {
                        Accessible.id: objectName
                        objectName: "panel.peopleView.accept" + "." + entry0.modelData.userId
                        text: qsTr("Accept")

                        onClicked: Models.People.accept(entry0.modelData.handle)
                    }
                    KButton {
                        Accessible.id: objectName
                        objectName: "panel.peopleView.decline" + "." + entry0.modelData.userId
                        text: qsTr("Decline")
                        variant: KButton.Quiet

                        onClicked: Models.People.decline(entry0.modelData.handle)
                    }
                }
            }
            Repeater {
                model: Models.People.outgoing

                delegate: RowLayout {
                    id: entry1

                    required property var modelData

                    Layout.fillWidth: true

                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.textSecondary
                        text: qsTr("Request sent to %1").arg(entry1.modelData.displayName || entry1.modelData.handle)
                    }
                    KButton {
                        Accessible.id: objectName
                        objectName: "panel.peopleView.cancel-request" + "." + entry1.modelData.userId
                        text: qsTr("Cancel")
                        variant: KButton.Quiet

                        onClicked: Models.People.cancel(entry1.modelData.handle)
                    }
                }
            }
            Repeater {
                model: Models.People.friends

                delegate: ColumnLayout {
                    id: entry2

                    readonly property bool changed: modelData.identityState === "changed"
                    required property var modelData
                    property bool verifying: false

                    Layout.fillWidth: true
                    spacing: 6

                    RowLayout {
                        Layout.fillWidth: true

                        PlainLabel {
                            Layout.fillWidth: true
                            color: KodosiTheme.textPrimary
                            text: entry2.modelData.displayName || entry2.modelData.handle
                        }
                        PlainLabel {
                            color: KodosiTheme.textSecondary
                            text: entry2.modelData.verified ? qsTr("Verified") : qsTr("Not verified")
                            visible: !entry2.changed
                        }
                        KButton {
                            Accessible.id: objectName
                            objectName: "panel.people.trust." + entry2.modelData.userId
                            text: qsTr("Trust")
                            visible: entry2.changed

                            onClicked: Models.People.trust(entry2.modelData.handle)
                        }
                        KButton {
                            Accessible.id: objectName
                            objectName: "panel.people.verify." + entry2.modelData.userId
                            text: qsTr("Verify…")
                            variant: KButton.Quiet
                            visible: entry2.changed || !entry2.modelData.verified

                            onClicked: entry2.verifying = !entry2.verifying
                        }
                        KButton {
                            Accessible.id: objectName
                            objectName: "panel.people.remove." + entry2.modelData.userId
                            text: qsTr("Remove…")
                            variant: KButton.Quiet

                            onClicked: {
                                removeFriend.username = entry2.modelData.handle;
                                removeFriend.open();
                            }
                        }
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.textSecondary
                        text: qsTr("Identity changed")
                        visible: entry2.changed
                        wrapMode: Text.WordWrap
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        visible: entry2.verifying

                        KTextField {
                            id: friendInvite

                            Accessible.id: objectName
                            Accessible.name: qsTr("Invite of this friend")
                            Layout.fillWidth: true
                            objectName: "panel.people.invite." + entry2.modelData.userId
                            placeholderText: qsTr("Paste the invite that this friend sent you")
                        }
                        KButton {
                            Accessible.id: objectName
                            enabled: friendInvite.text.trim().length > 0
                            objectName: "panel.people.verify-invite." + entry2.modelData.userId
                            text: qsTr("Verify")

                            onClicked: {
                                Models.People.verify(entry2.modelData.handle, friendInvite.text);
                                entry2.verifying = false;
                            }
                        }
                    }
                }
            }
            PlainLabel {
                color: KodosiTheme.textSecondary
                text: qsTr("No friends yet.")
                visible: Models.Account.signedIn && Models.Devices.localDeviceEnrolled && Models.People.friends.length === 0
            }
        }
    }
    KDialog {
        id: removeFriend

        property string username: ""

        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Remove %1?").arg(username)

        onAccepted: Models.People.remove(username)

        PlainLabel {
            color: KodosiTheme.textPrimary
            text: qsTr("They will lose access to terminals shared with them.")
        }
    }
}
