pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property var friends: Models.People.friends
    property bool copied: false

    function add() {
        if (handle.text.trim().length === 0)
            return;
        trust.askToAdd(handle.text);
    }

    Accessible.id: objectName
    objectName: "panel.people"

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(420, parent.width - 48)
        visible: !root.accountReady
        title: qsTr("Friends can join your terminals")
        message: qsTr("Add a friend by name. You choose each terminal that you share.")
        art: Row {
            spacing: -10
            PersonAvatar { name: "M"; key: "people-a"; size: 44; ring: KodosiTheme.surface }
            PersonAvatar { name: "K"; isSelf: true; size: 44; ring: KodosiTheme.surface }
            PersonAvatar { name: "D"; key: "people-c"; size: 44; ring: KodosiTheme.surface }
        }

        KButton {
            variant: KButton.Primary
            large: true
            text: Models.Account.signedIn ? qsTr("Approve this device") : qsTr("Sign in")
            objectName: "panel.people.signIn"
            Accessible.id: objectName
            onClicked: {
                Models.DesktopState.activeView = Models.DesktopState.Settings;
                if (!Models.Account.signedIn)
                    Models.Account.login();
            }
        }
    }
    KScrollView {
        id: peopleScroll
        anchors.fill: parent
        visible: root.accountReady
        contentWidth: availableWidth

        ColumnLayout {
            x: 36
            width: Math.min(1028, peopleScroll.availableWidth - 72)
            spacing: 22

            ColumnLayout {
                Layout.topMargin: 50
                spacing: 4
                PlainLabel { text: qsTr("Friends"); font.pixelSize: KodosiTheme.fontLarge; font.weight: Font.DemiBold; font.letterSpacing: -0.6 }
                PlainLabel { text: qsTr("%n friends", "", root.friends.length); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Item {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 440
                    implicitHeight: 40

                    Well { anchors.fill: parent; radius: 20 }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 14
                        anchors.rightMargin: 5
                        spacing: 8

                        KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "share"; color: KodosiTheme.inkFaint }
                        TextInput {
                            id: handle
                            Layout.fillWidth: true
                            color: KodosiTheme.ink
                            font.pixelSize: KodosiTheme.fontBody
                            selectionColor: KodosiTheme.accent
                            selectedTextColor: KodosiTheme.accentInk
                            clip: true
                            Accessible.id: objectName
                            Accessible.name: qsTr("Username or invite")
                            Accessible.role: Accessible.EditableText
                            objectName: "panel.people.username"
                            onAccepted: root.add()

                            PlainLabel { visible: handle.text.length === 0; text: qsTr("Add a friend by username or invite"); color: KodosiTheme.inkFaint }
                        }
                        KButton {
                            Accessible.id: objectName
                            compact: true
                            variant: KButton.Primary
                            enabled: handle.text.trim().length > 0
                            objectName: "panel.peopleView.add-friend"
                            text: qsTr("Add")

                            onClicked: root.add()
                        }
                    }
                }
                KButton {
                    Accessible.id: objectName
                    objectName: "panel.peopleView.copy-invite"
                    variant: KButton.Ghost
                    iconName: root.copied ? "check" : "copy"
                    text: root.copied ? qsTr("Copied") : qsTr("Copy my invite")

                    onClicked: Models.People.copyInvite()
                }
                Timer { id: copiedTimer; interval: 2000; onTriggered: root.copied = false }
                Connections {
                    target: Models.People
                    function onInviteCopied() {
                        root.copied = true;
                        copiedTimer.restart();
                    }
                }
                Item { Layout.fillWidth: true }
            }
            Repeater {
                model: Models.People.incoming

                delegate: Item {
                    id: incoming

                    required property var modelData
                    readonly property string fullName: modelData.displayName || modelData.handle

                    Layout.fillWidth: true
                    Layout.maximumWidth: 520
                    implicitHeight: 64

                    Raised { anchors.fill: parent; radius: KodosiTheme.radiusXl; fill: KodosiTheme.mix(KodosiTheme.raised, KodosiTheme.accentSoft, 0.6) }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 14
                        anchors.rightMargin: 12
                        spacing: 12

                        PersonAvatar { name: incoming.fullName; key: incoming.modelData.userId; size: 36 }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            PlainLabel { Layout.fillWidth: true; text: incoming.fullName; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold; elide: Text.ElideRight }
                            PlainLabel { text: qsTr("Wants to be your friend"); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote }
                        }
                        KButton {
                            Accessible.id: objectName
                            variant: KButton.Primary
                            objectName: "panel.peopleView.accept" + "." + incoming.modelData.userId
                            text: qsTr("Accept…")

                            onClicked: trust.askToAccept(incoming.modelData)
                        }
                        KButton {
                            Accessible.id: objectName
                            objectName: "panel.peopleView.decline" + "." + incoming.modelData.userId
                            text: qsTr("Decline")
                            variant: KButton.Ghost

                            onClicked: Models.People.decline(incoming.modelData.handle)
                        }
                        KButton {
                            Accessible.id: objectName
                            objectName: "panel.peopleView.block" + "." + incoming.modelData.userId
                            text: qsTr("Block…")
                            variant: KButton.Ghost

                            onClicked: blockPerson.ask(incoming.modelData.handle, incoming.fullName)
                        }
                    }
                }
            }
            Flow {
                Layout.fillWidth: true
                visible: Models.People.outgoing.length > 0
                spacing: 8

                Repeater {
                    model: Models.People.outgoing

                    delegate: Rectangle {
                        id: outgoing

                        required property var modelData

                        width: sent.implicitWidth + 18
                        height: 32
                        radius: 16
                        color: KodosiTheme.alpha(KodosiTheme.ink, 0.06)

                        RowLayout {
                            id: sent
                            x: 10
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6

                            PlainLabel {
                                color: KodosiTheme.inkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                                text: qsTr("Request sent to %1").arg(outgoing.modelData.displayName || outgoing.modelData.handle)
                            }
                            KIconButton {
                                Accessible.id: objectName
                                Accessible.name: qsTr("Cancel request")
                                glyph: "close"
                                size: 22
                                objectName: "panel.peopleView.cancel-request" + "." + outgoing.modelData.userId

                                onClicked: Models.People.cancel(outgoing.modelData.handle)
                            }
                        }
                    }
                }
            }
            CardGrid {
                id: grid
                Layout.fillWidth: true
                minimum: 290
                maximum: 400
                gap: 14

                Repeater {
                    model: root.friends

                    delegate: Item {
                        id: friend

                        readonly property bool changed: modelData.identityState === "changed"
                        readonly property string fullName: modelData.displayName || modelData.handle
                        readonly property var shared: Models.Sessions.listedSessions.filter(session => session.ownerUserId === modelData.userId && session.isOwner === false)
                        required property var modelData
                        property bool verifying: false

                        width: grid.cardWidth
                        height: body.implicitHeight + 28

                        Raised { anchors.fill: parent; radius: KodosiTheme.radiusXl }
                        ColumnLayout {
                            id: body
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 14
                            spacing: 12

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12

                                PersonAvatar { name: friend.fullName; key: friend.modelData.userId; size: 44 }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    PlainLabel { Layout.fillWidth: true; text: friend.fullName; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold; elide: Text.ElideRight }
                                    RowLayout {
                                        spacing: 6
                                        PlainLabel { text: "@" + friend.modelData.handle; color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote }
                                        Tag { visible: friend.modelData.verified === true && !friend.changed; text: qsTr("Verified"); iconName: "check"; tone: Tag.Ready }
                                        Tag { visible: friend.changed; text: qsTr("Identity changed"); iconName: "warning"; tone: Tag.Caution }
                                    }
                                }
                                KIconButton {
                                    id: more
                                    glyph: "more"
                                    Accessible.name: qsTr("Options for %1").arg(friend.fullName)
                                    onClicked: menu.popup(more, 0, more.height + 4)

                                    KMenu {
                                        id: menu
                                        KMenuItem {
                                            Accessible.id: objectName
                                            objectName: "panel.people.verify." + friend.modelData.userId
                                            text: qsTr("Verify…")
                                            iconName: "check"
                                            enabled: friend.changed || !friend.modelData.verified
                                            onTriggered: friend.verifying = true
                                        }
                                        KMenuItem {
                                            Accessible.id: objectName
                                            objectName: "panel.people.remove." + friend.modelData.userId
                                            text: qsTr("Remove…")
                                            iconName: "close"
                                            destructive: true
                                            onTriggered: {
                                                removeFriend.username = friend.modelData.handle;
                                                removeFriend.fullName = friend.fullName;
                                                removeFriend.open();
                                            }
                                        }
                                        KMenuItem {
                                            Accessible.id: objectName
                                            objectName: "panel.people.block." + friend.modelData.userId
                                            text: qsTr("Block…")
                                            iconName: "hand"
                                            destructive: true
                                            onTriggered: blockPerson.ask(friend.modelData.handle, friend.fullName)
                                        }
                                    }
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                visible: friend.changed
                                spacing: 8

                                PlainLabel {
                                    Layout.fillWidth: true
                                    text: friend.modelData.inviteNeeded === true
                                        ? qsTr("Their identity changed. Verify it with a new invite from them.")
                                        : qsTr("Their identity changed. Trust it only if they told you so.")
                                    color: KodosiTheme.inkMuted
                                    font.pixelSize: KodosiTheme.fontFootnote
                                    wrapMode: Text.WordWrap
                                }
                                KButton {
                                    Accessible.id: objectName
                                    compact: true
                                    variant: KButton.Tinted
                                    visible: friend.modelData.inviteNeeded !== true
                                    objectName: "panel.people.trust." + friend.modelData.userId
                                    text: qsTr("Trust")

                                    onClicked: Models.People.trust(friend.modelData.handle)
                                }
                                KButton {
                                    Accessible.id: objectName
                                    compact: true
                                    variant: KButton.Tinted
                                    visible: friend.modelData.inviteNeeded === true && !friend.verifying
                                    objectName: "panel.people.verify-changed." + friend.modelData.userId
                                    text: qsTr("Verify…")

                                    onClicked: friend.verifying = true
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                visible: friend.verifying
                                spacing: 6

                                KTextField {
                                    id: friendInvite

                                    Accessible.id: objectName
                                    Accessible.name: qsTr("Invite of this friend")
                                    Layout.fillWidth: true
                                    objectName: "panel.people.invite." + friend.modelData.userId
                                    placeholderText: qsTr("Paste the invite they sent you")
                                }
                                KButton {
                                    Accessible.id: objectName
                                    compact: true
                                    variant: KButton.Primary
                                    enabled: friendInvite.text.trim().length > 0
                                    objectName: "panel.people.verify-invite." + friend.modelData.userId
                                    text: qsTr("Verify")

                                    onClicked: {
                                        Models.People.verify(friend.modelData.handle, friendInvite.text);
                                        friend.verifying = false;
                                    }
                                }
                            }
                            Flow {
                                Layout.fillWidth: true
                                visible: friend.shared.length > 0
                                spacing: 6

                                Repeater {
                                    model: friend.shared

                                    AbstractButton {
                                        id: shared

                                        required property var modelData

                                        height: 28
                                        implicitWidth: sharedRow.implicitWidth + 18
                                        hoverEnabled: true
                                        Accessible.name: qsTr("Open %1").arg(modelData.name)
                                        contentItem: Item {
                                            RowLayout {
                                                id: sharedRow
                                                x: 6
                                                anchors.verticalCenter: parent.verticalCenter
                                                spacing: 6
                                                AgentMark { program: shared.modelData.program || ""; size: 18; session: shared.modelData }
                                                PlainLabel { text: shared.modelData.name; font.pixelSize: KodosiTheme.fontFootnote; font.weight: Font.Medium }
                                            }
                                        }
                                        background: Rectangle { radius: 14; color: KodosiTheme.alpha(KodosiTheme.ink, shared.hovered ? 0.1 : 0.06) }
                                        onClicked: Models.SessionActions.activate(modelData.id)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.maximumWidth: 520
                visible: Models.People.blocked.length > 0
                spacing: 8

                PlainLabel { text: qsTr("Blocked"); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontCallout; font.weight: Font.DemiBold }
                Repeater {
                    model: Models.People.blocked

                    delegate: RowLayout {
                        id: blocked

                        required property var modelData

                        Layout.fillWidth: true
                        spacing: 10

                        PersonAvatar { name: blocked.modelData.displayName || blocked.modelData.handle; key: blocked.modelData.userId; size: 28 }
                        PlainLabel { text: blocked.modelData.displayName || blocked.modelData.handle; elide: Text.ElideRight }
                        PlainLabel { Layout.fillWidth: true; text: "@" + blocked.modelData.handle; color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote; elide: Text.ElideRight }
                        KButton {
                            Accessible.id: objectName
                            compact: true
                            objectName: "panel.peopleView.unblock" + "." + blocked.modelData.userId
                            text: qsTr("Unblock")
                            variant: KButton.Ghost

                            onClicked: Models.People.unblock(blocked.modelData.handle)
                        }
                    }
                }
            }
            EmptyState {
                Layout.fillWidth: true
                Layout.topMargin: 30
                visible: root.friends.length === 0 && Models.People.incoming.length === 0
                title: qsTr("No friends yet")
                message: qsTr("Send your invite, or add a username above.")
            }
            Item { Layout.preferredHeight: 30 }
        }
    }
    FriendTrustDialog {
        id: trust
        onAdded: handle.clear()
    }
    KDialog {
        id: blockPerson

        property string username: ""
        property string fullName: ""

        function ask(handle, name) {
            username = handle;
            fullName = name;
            open();
        }

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Block %1?").arg(fullName)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Block")

        onAccepted: Models.People.block(username)

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            text: qsTr("They cannot send you friend requests. If you are friends, you stop sharing terminals with each other, and each of you leaves the rooms that the other owns.")
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: removeFriend

        property string username: ""
        property string fullName: ""

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Remove %1?").arg(fullName)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Remove")

        onAccepted: Models.People.remove(username)

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            text: qsTr("You stop sharing terminals with each other, and each of you leaves the rooms that the other owns. Kodosi does not undo the commands that they already ran.")
            wrapMode: Text.WordWrap
        }
    }
}
