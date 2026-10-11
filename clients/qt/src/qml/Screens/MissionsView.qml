pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property bool selected: Models.Missions.selectedMissionId.length > 0
    readonly property var rooms: Models.Missions.missions
    readonly property var invitations: Models.Missions.invitations

    signal inspectSessionRequested(string sessionId)
    signal shareSessionRequested(string sessionId)

    function openCreate() {
        create.open();
    }

    objectName: "panel.missions"
    Accessible.id: objectName

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(420, parent.width - 48)
        visible: !root.accountReady
        title: qsTr("Rooms are for working together")
        message: qsTr("Share terminals, talk, and hand off tasks with people and agents.")
        art: Row {
            spacing: -10
            PersonAvatar { name: "A"; key: "together-a"; size: 44; ring: KodosiTheme.surface }
            AgentMark { program: "claude"; size: 44 }
            PersonAvatar { name: "K"; isSelf: true; size: 44; ring: KodosiTheme.surface }
        }

        KButton {
            variant: KButton.Primary
            large: true
            text: !Models.Account.signedIn ? qsTr("Sign in") : qsTr("Approve this device")
            objectName: "missions.signIn"
            Accessible.id: objectName
            onClicked: {
                Models.DesktopState.activeView = Models.DesktopState.Settings;
                if (!Models.Account.signedIn)
                    Models.Account.login();
            }
        }
    }
    KScrollView {
        id: lobby
        anchors.fill: parent
        visible: root.accountReady && !root.selected
        contentWidth: availableWidth

        ColumnLayout {
            x: 36
            width: Math.min(1028, lobby.availableWidth - 72)
            spacing: 24

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 50

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    PlainLabel { text: qsTr("Rooms"); font.pixelSize: KodosiTheme.fontLarge; font.weight: Font.DemiBold; font.letterSpacing: -0.6 }
                    PlainLabel {
                        text: Identity.count(root.rooms.length, qsTr("1 room"), qsTr("%1 rooms"))
                        color: KodosiTheme.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                    }
                }
                KButton {
                    variant: KButton.Primary
                    iconName: "plus"
                    text: qsTr("New room")
                    enabled: root.accountReady
                    objectName: "missions.create"
                    Accessible.id: objectName
                    onClicked: create.open()
                }
            }
            PlainLabel {
                Layout.fillWidth: true
                visible: Models.Missions.catalogTruncated
                text: qsTr("More rooms exist. Use Go to… to find them.")
                color: KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontFootnote
                objectName: "panel.missions.truncated"
                Accessible.id: objectName
                Accessible.name: qsTr("Some rooms are not shown")
            }
            Repeater {
                model: root.invitations

                Item {
                    id: invitation

                    required property var modelData

                    Layout.fillWidth: true
                    Layout.maximumWidth: 520
                    implicitHeight: 64

                    Raised { anchors.fill: parent; radius: KodosiTheme.radiusXl; fill: KodosiTheme.mix(KodosiTheme.raised, KodosiTheme.accentSoft, 0.6) }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 14
                        anchors.rightMargin: 12
                        spacing: 12

                        RoomSigil { key: invitation.modelData.missionId || ""; size: 36 }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            PlainLabel { Layout.fillWidth: true; text: invitation.modelData.missionName || ""; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold; elide: Text.ElideRight }
                            PlainLabel { text: qsTr("You are invited"); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote }
                        }
                        KButton {
                            variant: KButton.Primary
                            text: qsTr("Join")
                            objectName: "panel.missionsView.accept." + invitation.modelData.id
                            Accessible.id: objectName
                            onClicked: Models.Missions.acceptInvitation(invitation.modelData.id)
                        }
                        KButton {
                            variant: KButton.Ghost
                            text: qsTr("Not now")
                            objectName: "panel.missionsView.decline." + invitation.modelData.id
                            Accessible.id: objectName
                            Accessible.name: qsTr("Decline invitation")
                            onClicked: Models.Missions.declineInvitation(invitation.modelData.id)
                        }
                    }
                }
            }
            CardGrid {
                id: grid
                Layout.fillWidth: true
                gap: 14

                Repeater {
                    model: root.rooms

                    AbstractButton {
                        id: card

                        required property var modelData

                        width: grid.cardWidth
                        height: 132
                        hoverEnabled: true
                        activeFocusOnTab: true
                        objectName: "missions.room." + modelData.id
                        Accessible.id: objectName
                        Accessible.name: modelData.name
                        scale: pressed ? 0.98 : 1

                        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

                        background: Raised {
                            radius: KodosiTheme.radiusXl
                            fill: card.hovered ? KodosiTheme.lifted : KodosiTheme.raised
                            elevation: card.hovered ? 2 : 1
                        }
                        contentItem: Item {
                            RoomSigil { x: 16; y: 16; key: card.modelData.id; size: 40 }
                            ColumnLayout {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                anchors.margins: 16
                                spacing: 3
                                PlainLabel { Layout.fillWidth: true; text: card.modelData.name; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold; elide: Text.ElideRight }
                                PlainLabel {
                                    text: card.modelData.ownerUserId === Models.Account.userId ? qsTr("Your room") : qsTr("Shared with you")
                                    color: KodosiTheme.inkMuted
                                    font.pixelSize: KodosiTheme.fontFootnote
                                }
                            }
                            KIcon { anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 18; width: 14; height: 14; name: "arrow-right"; color: KodosiTheme.inkFaint; opacity: card.hovered ? 1 : 0 }
                        }
                        onClicked: Models.Missions.open(modelData.id)
                    }
                }
                AbstractButton {
                    id: slot
                    width: grid.cardWidth
                    height: 132
                    hoverEnabled: true
                    activeFocusOnTab: true
                    objectName: "missions.create.slot"
                    Accessible.id: objectName
                    Accessible.name: label.text
                    background: Rectangle {
                        radius: KodosiTheme.radiusXl
                        color: KodosiTheme.alpha(KodosiTheme.ink, slot.hovered ? 0.05 : 0)
                        border.width: 1
                        border.color: KodosiTheme.alpha(KodosiTheme.inkFaint, slot.hovered ? 0.8 : 0.45)
                    }
                    contentItem: Item {
                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: 10
                            KIcon { Layout.alignment: Qt.AlignHCenter; Layout.preferredWidth: 18; Layout.preferredHeight: 18; name: "plus"; strokeWidth: 2.2 }
                            PlainLabel { id: label; text: root.rooms.length === 0 ? qsTr("Make your first room") : qsTr("New room"); color: KodosiTheme.inkMuted; font.weight: Font.DemiBold }
                        }
                    }
                    onClicked: create.open()
                }
            }
            Item { Layout.preferredHeight: 30 }
        }
    }
    RoomContent {
        anchors.fill: parent
        visible: root.accountReady && root.selected
        onInspectSessionRequested: id => root.inspectSessionRequested(id)
        onShareSessionRequested: id => root.shareSessionRequested(id)
    }
    KPopover {
        id: create
        objectName: "room.create.popup"
        modal: true
        focus: true
        parent: Overlay.overlay
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        width: 380
        padding: 22
        radius: KodosiTheme.radiusSheet
        elevation: 3
        onOpened: name.forceActiveFocus()

        contentItem: ColumnLayout {
            spacing: 16

            RowLayout {
                spacing: 12
                RoomSigil { key: name.text.trim().length > 0 ? name.text.trim() : "new-room"; size: 40 }
                PlainLabel { Layout.fillWidth: true; text: qsTr("New room"); font.pixelSize: KodosiTheme.fontTitle; font.weight: Font.DemiBold }
            }
            KTextField {
                id: name
                Layout.fillWidth: true
                placeholderText: qsTr("Room name")
                objectName: "room.create.name"
                Accessible.id: objectName
                onAccepted: {
                    if (submit.enabled)
                        submit.clicked();
                }
            }
            RowLayout {
                spacing: 8
                Item { Layout.fillWidth: true }
                KButton { text: qsTr("Cancel"); variant: KButton.Ghost; onClicked: create.close() }
                KButton {
                    id: submit
                    text: qsTr("Create room")
                    variant: KButton.Primary
                    working: Models.Missions.busy
                    enabled: name.text.trim().length > 0 && !Models.Missions.busy
                    objectName: "room.create"
                    Accessible.id: objectName
                    onClicked: Models.Missions.create(name.text)
                }
            }
        }
    }
    Connections {
        target: Models.Missions
        function onCreated(roomId) {
            name.clear();
            create.close();
        }
    }
}
