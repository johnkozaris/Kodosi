pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root
    readonly property bool selected: Models.Missions.selectedMissionId.length > 0
    readonly property var presentation: Models.Missions.presentation
    property bool creating: false
    signal inspectSessionRequested(string sessionId)
    objectName: "panel.missions"
    Accessible.id: objectName
    RowLayout {
        anchors.fill: parent
        spacing: 0
        Rectangle {
            Layout.preferredWidth: 184; Layout.fillHeight: true
            color: KodosiTheme.surface
            ColumnLayout {
                anchors.fill: parent; spacing: 6
                RowLayout {
                    Layout.fillWidth: true; Layout.margins: 12
                    PlainLabel { text: qsTr("Rooms"); font.weight: Font.DemiBold; Layout.fillWidth: true }
                    KIcon { name: "more"; visible: Models.Missions.catalogTruncated; Accessible.name: qsTr("Room list limited"); objectName: "panel.missions.truncated" }
                    KIconButton { glyph: "plus"; Accessible.name: qsTr("New room"); onClicked: create.open(); enabled: Models.Account.signedIn && Models.Devices.localDeviceEnrolled; objectName: "missions.create" }
                }
                ListView {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    clip: true; model: Models.Missions.missions; spacing: 4
                    ScrollBar.vertical: KScrollBar {}
                    delegate: ItemDelegate {
                        id: roomRow
                        required property var modelData
                        width: ListView.view.width
                        height: 52
                        Accessible.name: modelData.name
                        objectName: "missions.room." + modelData.id
                        background: Rectangle { anchors.fill: parent; anchors.margins: 4; radius: 9; color: Models.Missions.selectedMissionId === roomRow.modelData.id ? KodosiTheme.surfaceSelected : roomRow.hovered ? KodosiTheme.surfaceRaised : "transparent"; Behavior on color { ColorAnimation { duration: KodosiTheme.motionFast } } }
                        contentItem: RowLayout {
                            spacing: 10
                            RoomAvatar { name: roomRow.modelData.name; implicitWidth: 26; implicitHeight: 26 }
                            PlainLabel { text: roomRow.modelData.name; Layout.fillWidth: true; elide: Text.ElideRight; font.pixelSize: 12; font.weight: Models.Missions.selectedMissionId === roomRow.modelData.id ? Font.DemiBold : Font.Normal }
                        }
                        onClicked: Models.Missions.open(modelData.id)
                    }
                }
                Repeater {
                    model: Models.Missions.invitations
                    delegate: ColumnLayout {
                        id: invitationRow
                        required property var modelData
                        Layout.fillWidth: true; Layout.margins: 12
                        PlainLabel { text: invitationRow.modelData.missionName; Layout.fillWidth: true; elide: Text.ElideRight; font.weight: Font.DemiBold }
                        RowLayout {
                            KButton { text: qsTr("Join"); variant: KButton.Primary; onClicked: Models.Missions.acceptInvitation(invitationRow.modelData.id) }
                            KIconButton { glyph: "close"; Accessible.name: qsTr("Decline invitation"); objectName: "panel.missionsView.decline." + invitationRow.modelData.id; onClicked: Models.Missions.declineInvitation(invitationRow.modelData.id) }
                        }
                    }
                }
            }
        }
        Rectangle { Layout.fillHeight: true; implicitWidth: 1; color: KodosiTheme.seam }
        ColumnLayout {
            Layout.fillWidth: true; Layout.fillHeight: true; spacing: 0
            visible: root.selected
            Rectangle {
                Layout.fillWidth: true; Layout.preferredHeight: 56; color: KodosiTheme.surface
                RowLayout {
                    anchors.fill: parent; anchors.leftMargin: 18; anchors.rightMargin: 12; spacing: 12
                    PlainLabel { text: Models.Missions.selectedMission.name || ""; font.pixelSize: 16; font.weight: Font.DemiBold; elide: Text.ElideRight; Layout.fillWidth: true }
                    KIconButton { glyph: "warning"; visible: !!root.presentation.failure; Accessible.name: qsTr("Room action failed"); onClicked: failure.open() }
                    ItemDelegate {
                        implicitWidth: avatars.implicitWidth; implicitHeight: 32
                        Accessible.name: qsTr("Room members"); objectName: "room.people"
                        background: null
                        contentItem: Row {
                            id: avatars; spacing: -6
                            Repeater { model: Models.Missions.members.slice(0, 4); delegate: RoomAvatar { required property var modelData; name: modelData.displayName || modelData.handle; border.color: KodosiTheme.surface; border.width: 2 } }
                        }
                        onClicked: people.open()
                    }
                    KIconButton { glyph: "sessions"; Accessible.name: qsTr("Add terminal"); onClicked: share.open(); objectName: "room.terminal.add" }
                    KIconButton { glyph: "chat"; Accessible.name: qsTr("Toggle conversation"); onClicked: Models.Missions.setPresentation("conversation", root.presentation.conversation === false); objectName: "room.conversation.toggle" }
                    KIconButton { glyph: "more"; Accessible.name: qsTr("Room options"); onClicked: options.open() }
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: KodosiTheme.seam }
            RoomContent { Layout.fillWidth: true; Layout.fillHeight: true; onInspectSessionRequested: id => root.inspectSessionRequested(id) }
        }
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true
            visible: !root.selected
            ColumnLayout {
                anchors.centerIn: parent; spacing: 20
                KIcon { name: "sessions"; implicitWidth: 54; implicitHeight: 54; color: KodosiTheme.accent; Layout.alignment: Qt.AlignHCenter }
                KButton {
                    text: !Models.Account.signedIn ? qsTr("Sign in") : !Models.Devices.localDeviceEnrolled ? qsTr("Connect device") : qsTr("New room")
                    variant: KButton.Primary
                    onClicked: {
                        if (!Models.Account.signedIn) Models.Account.login();
                        else if (!Models.Devices.localDeviceEnrolled) Models.DesktopState.activeView = Models.DesktopState.Settings;
                        else create.open();
                    }
                }
            }
        }
    }
    KPopover {
        id: create
        objectName: "room.create.popup"
        parent: Overlay.overlay; x: (parent.width - width) / 2; y: (parent.height - height) / 2; width: 360; padding: 20
        contentItem: ColumnLayout {
            spacing: 14
            KTextField { id: name; Layout.fillWidth: true; placeholderText: qsTr("Room name"); objectName: "room.create.name" }
            RowLayout {
                KButton { text: qsTr("Cancel"); onClicked: create.close() }
                Item { Layout.fillWidth: true }
                KButton { text: qsTr("Create room"); variant: KButton.Primary; enabled: name.text.trim().length > 0 && !Models.Missions.busy; onClicked: { Models.Missions.create(name.text); root.creating = true; } objectName: "room.create" }
            }
        }
    }
    Connections {
        target: Models.Missions
        function onCreated(roomId) { root.creating = false; name.clear(); create.close(); }
        function onBusyChanged() { if (!Models.Missions.busy) root.creating = false; }
    }
    KPopover {
        id: people
        x: Math.max(0, root.width - width - 18); y: 48; width: 290; padding: 18
        contentItem: ColumnLayout {
            spacing: 12
            Repeater {
                model: Models.Missions.members
                delegate: RowLayout {
                    id: memberRow
                    required property var modelData
                    Layout.fillWidth: true
                    RoomAvatar { name: memberRow.modelData.displayName || memberRow.modelData.handle }
                    PlainLabel { text: memberRow.modelData.displayName || memberRow.modelData.handle; Layout.fillWidth: true; elide: Text.ElideRight }
                    KIconButton { glyph: "close"; visible: Models.Missions.selectedMission.ownerUserId === Models.Account.userId && !memberRow.modelData.isOwner; Accessible.name: qsTr("Remove %1 from room").arg(memberRow.modelData.displayName); onClicked: Models.Missions.removeMember(memberRow.modelData.userId) }
                }
            }
            RowLayout {
                visible: Models.Missions.selectedMission.ownerUserId === Models.Account.userId
                KComboBox { id: friend; Layout.fillWidth: true; model: Models.People.friends; textRole: "displayName"; valueRole: "userId" }
                KIconButton { glyph: "plus"; Accessible.name: qsTr("Invite"); enabled: friend.currentIndex >= 0; onClicked: Models.Missions.invite(friend.currentValue) }
            }
        }
    }
    KPopover {
        id: share
        x: Math.max(0, root.width - width - 18); y: 48; width: 290; padding: 12
        contentItem: ColumnLayout {
            spacing: 6
            Repeater {
                model: Models.Sessions.folderGroups
                delegate: ColumnLayout {
                    id: sessionGroup
                    required property var modelData
                    Repeater {
                        model: sessionGroup.modelData.sessions
                        delegate: KButton {
                            required property var modelData
                            readonly property var info: Models.Sessions.presentationForSession(modelData.id)
                            visible: !!info.isOwner && info.missionId !== Models.Missions.selectedMissionId
                            text: modelData.name; iconName: "terminal"; Layout.fillWidth: true; variant: KButton.Quiet
                            onClicked: { Models.SessionActions.attachMission(modelData.id, Models.Missions.selectedMissionId); share.close(); }
                        }
                    }
                }
            }
            KButton { text: qsTr("New terminal"); iconName: "plus"; Layout.fillWidth: true; variant: KButton.Primary; onClicked: { Models.SessionActions.createInRoom(Models.Missions.selectedMissionId, Models.DesktopSettings.effectiveWorkingDirectory); share.close(); } }
        }
    }
    KPopover {
        id: options
        x: Math.max(0, root.width - width - 18); y: 48; width: 290; padding: 16
        contentItem: ColumnLayout {
            spacing: 12
            KTextField { id: rename; text: Models.Missions.selectedMission.name || ""; visible: Models.Missions.selectedMission.ownerUserId === Models.Account.userId; Layout.fillWidth: true }
            KButton { text: qsTr("Rename"); visible: rename.visible; onClicked: { Models.Missions.rename(rename.text); options.close(); } }
            KButton { text: qsTr("Refresh"); onClicked: { Models.Missions.roomAction({type: "read"}); options.close(); } }
            KButton { text: rename.visible ? qsTr("Delete room…") : qsTr("Leave room"); onClicked: { options.close(); if (rename.visible) remove.open(); else Models.Missions.leave(); } }
        }
    }
    KDialog { id: remove; title: qsTr("Delete room?"); standardButtons: Dialog.Ok | Dialog.Cancel; onAccepted: Models.Missions.remove() }
    KPopover {
        id: failure
        x: Math.max(0, root.width - width - 18); y: 48; width: 320; padding: 16
        contentItem: ColumnLayout {
            spacing: 12
            KReadOnlyText { Layout.fillWidth: true; text: root.presentation.failure || "" }
            KButton { text: qsTr("Try again"); onClicked: { Models.Missions.roomAction({type: "read"}); failure.close(); } }
        }
    }
}
