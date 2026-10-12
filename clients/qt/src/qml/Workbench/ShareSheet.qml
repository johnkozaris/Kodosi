pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    property string sessionId: ""
    property var session: ({})
    property var people: []
    property var originalPeople: []
    property string room: ""
    readonly property string currentRoom: session.missionId || ""
    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property bool changed: room !== currentRoom || people.slice().sort().join() !== originalPeople.slice().sort().join()
    readonly property var here: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)
    readonly property string actionTitle: {
        if (room !== currentRoom) {
            const target = Models.Missions.missions.find(mission => mission.id === room)
            return target ? qsTr("Share with %1").arg(target.name) : qsTr("Stop sharing")
        }
        if (!changed && (room.length > 0 || people.length > 0))
            return qsTr("Shared")
        if (people.length === 0)
            return originalPeople.length === 0 ? qsTr("Share") : qsTr("Stop sharing")
        if (people.length === 1)
            return qsTr("Share with %1").arg(Identity.personName(people[0]))
        return qsTr("Share with %1 people").arg(people.length)
    }

    function openSession(id) {
        sessionId = id;
        session = Models.Sessions.presentationForSession(id);
        originalPeople = (session.sharedWith || []).slice();
        people = originalPeople.slice();
        room = currentRoom;
        open();
    }
    function toggle(userId) {
        const next = people.slice();
        const index = next.indexOf(userId);
        if (index >= 0)
            next.splice(index, 1);
        else
            next.push(userId);
        people = next;
    }
    function save() {
        const sent = room !== currentRoom
            ? Models.SessionActions.attachMission(sessionId, room)
            : Models.SessionActions.share(sessionId, people, originalPeople);
        if (sent)
            close();
    }

    focus: true
    modal: true
    objectName: "panel.share"
    padding: 20
    parent: Overlay.overlay
    radius: KodosiTheme.radiusSheet
    elevation: 3
    width: Math.min(380, parent ? parent.width - 32 : 380)
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0

    Connections {
        function onModelReset() {
            root.session = Models.Sessions.presentationForSession(root.sessionId);
            if (!root.session.sessionId)
                root.close();
        }

        target: Models.Sessions
    }
    contentItem: ColumnLayout {
        spacing: 18

        RowLayout {
            spacing: 11

            AgentMark { program: root.session.program || ""; size: 34 }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                PlainLabel {
                    Layout.fillWidth: true
                    text: qsTr("Share %1").arg(root.session.name || "")
                    font.pixelSize: KodosiTheme.fontHeadline
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                RowLayout {
                    spacing: 5
                    KIcon { Layout.preferredWidth: 11; Layout.preferredHeight: 11; name: "lock"; strokeWidth: 2.2 }
                    PlainLabel { text: qsTr("Encrypted end to end"); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
                }
            }
            KIconButton {
                glyph: "close"
                objectName: "sharing.close"
                Accessible.id: objectName
                Accessible.name: qsTr("Close")
                onClicked: root.close()
            }
        }
        RowLayout {
            visible: root.here.length > 0
            spacing: 8

            AvatarStack { userIds: root.here; size: 22; ring: KodosiTheme.lifted }
            PlainLabel { text: qsTr("Here now"); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote }
        }
        KButton {
            Layout.fillWidth: true
            visible: !root.accountReady
            variant: KButton.Primary
            text: Models.Account.signedIn ? qsTr("Approve this device") : qsTr("Sign in")
            onClicked: {
                root.close();
                Models.DesktopState.activeView = Models.DesktopState.Settings;
                if (!Models.Account.signedIn)
                    Models.Account.login();
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.accountReady && Models.Missions.missions.length > 0
            spacing: 8

            PlainLabel { text: qsTr("A room"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
            Flow {
                Layout.fillWidth: true
                spacing: 6

                Repeater {
                    model: Models.Missions.missions

                    AbstractButton {
                        id: chip

                        required property var modelData
                        readonly property bool selected: root.room === modelData.id

                        height: 28
                        implicitWidth: chipRow.implicitWidth + 16
                        scale: pressed ? 0.96 : 1
                        objectName: "sharing.room." + modelData.id
                        Accessible.id: objectName
                        Accessible.name: modelData.name
                        Accessible.selected: selected
                        contentItem: Item {
                            RowLayout {
                                id: chipRow
                                x: 5
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 7
                                RoomSigil { key: chip.modelData.id; size: 18 }
                                PlainLabel { text: chip.modelData.name; color: chip.selected ? KodosiTheme.accentStrong : KodosiTheme.ink; font.pixelSize: KodosiTheme.fontFootnote; font.weight: Font.Medium }
                                KIcon { visible: chip.selected; Layout.preferredWidth: 10; Layout.preferredHeight: 10; name: "check"; strokeWidth: 2.8; color: KodosiTheme.accentStrong }
                            }
                        }
                        background: Rectangle {
                            radius: 14
                            color: chip.selected ? KodosiTheme.accentSoft : KodosiTheme.alpha(KodosiTheme.ink, 0.07)
                            border.width: chip.selected ? 1 : 0
                            border.color: KodosiTheme.alpha(KodosiTheme.accent, 0.6)
                        }
                        onClicked: {
                            root.room = selected ? "" : modelData.id;
                            root.people = root.originalPeople.slice();
                        }
                    }
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.accountReady && root.room.length === 0 && root.currentRoom.length === 0
            spacing: 8

            PlainLabel { text: qsTr("People"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
            KButton {
                visible: Models.People.friends.length === 0
                variant: KButton.Tinted
                iconName: "share"
                text: qsTr("Add a friend")
                onClicked: { root.close(); Models.DesktopState.activeView = Models.DesktopState.People; }
            }
            Flow {
                Layout.fillWidth: true
                spacing: 6

                Repeater {
                    model: Models.People.friends

                    AbstractButton {
                        id: friend

                        required property var modelData
                        readonly property bool selected: root.people.indexOf(modelData.userId) >= 0
                        readonly property bool blocked: modelData.identityState === "changed" && !selected
                        readonly property string fullName: modelData.displayName || modelData.handle

                        width: 62
                        height: 66
                        enabled: !blocked
                        opacity: blocked ? 0.45 : 1
                        scale: pressed ? 0.95 : 1
                        objectName: "sharing.friend." + modelData.userId
                        Accessible.id: objectName
                        Accessible.name: fullName
                        Accessible.selected: selected
                        ToolTip.visible: hovered
                        ToolTip.text: blocked ? qsTr("Their identity changed. Trust them in People first.") : fullName
                        ToolTip.delay: 500
                        hoverEnabled: true
                        contentItem: Item {
                            Rectangle {
                                anchors.centerIn: avatar
                                width: 48
                                height: 48
                                radius: 24
                                color: "transparent"
                                border.width: 2.5
                                border.color: KodosiTheme.accent
                                visible: friend.selected
                            }
                            PersonAvatar { id: avatar; anchors.horizontalCenter: parent.horizontalCenter; y: 4; name: friend.fullName; key: friend.modelData.userId; size: 40 }
                            Rectangle {
                                visible: friend.selected
                                x: avatar.x + 28
                                y: avatar.y + 28
                                width: 16
                                height: 16
                                radius: 8
                                color: KodosiTheme.accent
                                border.width: 2
                                border.color: KodosiTheme.lifted
                                KIcon { anchors.centerIn: parent; width: 8; height: 8; name: "check"; strokeWidth: 3.4; color: KodosiTheme.accentInk }
                            }
                            PlainLabel {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.bottom: parent.bottom
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                text: Identity.firstName(friend.fullName)
                                color: friend.selected ? KodosiTheme.ink : KodosiTheme.inkMuted
                                font.pixelSize: KodosiTheme.fontCaption
                                font.weight: Font.Medium
                                elide: Text.ElideRight
                            }
                        }
                        onClicked: root.toggle(modelData.userId)
                    }
                }
            }
        }
        KButton {
            Layout.fillWidth: true
            visible: root.accountReady
            variant: KButton.Primary
            large: true
            text: root.actionTitle
            enabled: root.changed && !Models.SessionActions.busy
            objectName: "sharing.save"
            Accessible.id: objectName
            onClicked: root.save()
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: root.accountReady
            text: qsTr("Everyone you share with can type, resize and close this terminal.")
            color: KodosiTheme.inkFaint
            font.pixelSize: KodosiTheme.fontCaption
            wrapMode: Text.WordWrap
        }
    }
}
