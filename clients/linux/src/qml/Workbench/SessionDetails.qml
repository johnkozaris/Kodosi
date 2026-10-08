pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    property var selectedUsers: []
    property var originalUsers: []
    property string selectedMission: ""
    property var session: ({})
    property string sessionId: ""

    function openSession(id) {
        sessionId = id;
        refresh();
        originalUsers = session.sharedWith || [];
        selectedUsers = originalUsers.slice();
        selectedMission = session.missionId || "";
        open();
    }
    function refresh() {
        session = Models.Sessions.presentationForSession(sessionId);
        if (JSON.stringify(selectedUsers.slice().sort()) === JSON.stringify((session.sharedWith || []).slice().sort()))
            originalUsers = (session.sharedWith || []).slice();
    }

    focus: true
    height: Math.min(600, parent ? parent.height - 32 : 600)
    modal: true
    objectName: "panel.sessionDetails"
    padding: 18
    parent: Overlay.overlay
    width: Math.min(460, parent ? parent.width - 32 : 460)
    x: parent ? parent.width - width - 12 : 0
    y: parent ? (parent.height - height) / 2 : 0

    Connections {
        function onModelReset() {
            root.refresh();
            if (!root.session.sessionId)
                root.close();
        }

        target: Models.Sessions
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        RowLayout {
            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.textPrimary
                elide: Text.ElideRight
                font.pixelSize: 18
                text: root.session.name || qsTr("Terminal")
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Close terminal details")
                glyph: "close"
                objectName: "panel.sessionDetails.close"

                onClicked: root.close()
            }
        }
        PlainLabel {
            color: KodosiTheme.textSecondary
            text: root.session.kind === "local" ? qsTr("This computer") : (root.session.hostName || root.session.ownerName || qsTr("Remote computer"))
        }
        PlainLabel { Layout.fillWidth: true; text: root.session.displayDirectory || ""; color: KodosiTheme.textSecondary; wrapMode: Text.WrapAnywhere }
        KButton {
            Accessible.id: objectName
            objectName: "panel.sessionDetails.open-project-folder"
            text: qsTr("Open project folder")
            visible: root.session.kind === "local" && !!root.session.workingDirectory

            onClicked: Models.DesktopFiles.openPath(root.session.workingDirectory)
        }
        PlainLabel { text: qsTr("Mission"); color: KodosiTheme.textPrimary; font.weight: Font.DemiBold }
        RowLayout {
            Layout.fillWidth: true
            visible: root.session.isOwner === true
            KComboBox {
                id: mission
                Accessible.id: objectName
                Accessible.name: qsTr("Mission")
                Layout.fillWidth: true
                model: [{ id: "", name: qsTr("Not shared with a room") }].concat(Models.Missions.missions)
                objectName: "panel.sessionDetails.mission"
                textRole: "name"
                valueRole: "id"
                currentIndex: { for (let i = 0; i < model.length; ++i) if (model[i].id === root.selectedMission) return i; return 0; }
                onActivated: root.selectedMission = currentValue
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.sessionDetails.mission.save"
                text: qsTr("Save")
                visible: root.selectedMission !== (root.session.missionId || "")
                enabled: !Models.SessionActions.busy
                onClicked: Models.SessionActions.attachMission(root.sessionId, root.selectedMission)
            }
        }
        PlainLabel { text: root.session.missionName || qsTr("Not shared with a room"); color: KodosiTheme.textSecondary; visible: root.session.isOwner !== true }
        PlainLabel {
            color: KodosiTheme.textPrimary
            font.weight: Font.DemiBold
            text: qsTr("People")
            visible: root.session.isOwner === true && root.session.kind === "local" && Models.Account.signedIn && Models.Devices.localDeviceEnrolled
        }
        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.textSecondary
            text: root.session.missionId ? qsTr("Everyone in the room can control this terminal, including people invited later.") : qsTr("Selected friends can control the full terminal.")
            visible: root.session.isOwner === true && root.session.kind === "local" && Models.Account.signedIn && Models.Devices.localDeviceEnrolled
            wrapMode: Text.WordWrap
        }
        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.textSecondary
            text: qsTr("Approve this device to share this terminal.")
            visible: root.session.isOwner === true && root.session.kind === "local" && Models.Account.signedIn && !Models.Devices.localDeviceEnrolled
            wrapMode: Text.WordWrap
        }
        PlainLabel {
            Layout.fillWidth: true
            text: qsTr("Connected")
            color: KodosiTheme.textSecondary
            visible: (root.session.connectedUsers || []).filter(user => user !== Models.Account.userId).length > 0
        }
        Repeater {
            model: (root.session.connectedUsers || []).filter(user => user !== Models.Account.userId)
            delegate: PlainLabel {
                required property string modelData
                color: KodosiTheme.textPrimary
                text: Models.People.displayName(modelData) || qsTr("Connected person")
            }
        }
        ListView {
            id: friends

            Layout.fillHeight: true
            Layout.fillWidth: true
            clip: true
            model: Models.People.friends
            visible: !root.session.missionId && root.session.isOwner === true && root.session.kind === "local" && Models.Account.signedIn && Models.Devices.localDeviceEnrolled

            delegate: KCheckBox {
                required property var modelData

                Accessible.id: objectName
                checked: root.selectedUsers.indexOf(modelData.userId) >= 0
                objectName: "panel.sessionDetails.friend." + modelData.userId
                enabled: checked || modelData.identityState !== "changed"
                text: modelData.identityState === "changed" ? qsTr("%1 (trust in People first)").arg(modelData.displayName || modelData.handle) : modelData.displayName || modelData.handle
                width: friends.width

                onToggled: {
                    const users = root.selectedUsers.slice();
                    const index = users.indexOf(modelData.userId);
                    if (checked && index < 0)
                        users.push(modelData.userId);
                    if (!checked && index >= 0)
                        users.splice(index, 1);
                    root.selectedUsers = users;
                }
            }
        }
        KButton {
            Accessible.id: objectName
            enabled: !Models.SessionActions.busy && JSON.stringify(root.selectedUsers.slice().sort()) !== JSON.stringify(root.originalUsers.slice().sort())
            objectName: "panel.sessionDetails.share"
            text: qsTr("Save")
            visible: !root.session.missionId && root.session.isOwner === true && root.session.kind === "local" && Models.Account.signedIn && Models.Devices.localDeviceEnrolled

            onClicked: Models.SessionActions.share(root.sessionId, root.selectedUsers, root.originalUsers)
        }
        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.textSecondary
            text: qsTr("Sharing can only be changed on the host.")
            visible: root.session.isOwner === true && root.session.kind === "remote"
            wrapMode: Text.WordWrap
        }
        Item {
            Layout.fillHeight: true
            visible: root.session.kind !== "local"
        }
        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.textSecondary
            text: root.session.message || ""
            visible: !!root.session.message
            wrapMode: Text.WordWrap
        }
        KButton {
            Accessible.id: objectName
            objectName: "panel.sessionDetails.leave-shared-session"
            text: qsTr("Leave shared terminal…")
            variant: KButton.Quiet
            visible: root.session.isOwner === false && !root.session.missionId

            onClicked: leaveConfirmation.open()
        }
    }
    KDialog {
        id: leaveConfirmation

        objectName: "panel.sessionDetails.leave-shared-session.confirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Leave this shared terminal?")

        onAccepted: {
            Models.SessionActions.leave(root.sessionId);
            root.close();
        }
        onOpened: standardButton(Dialog.Ok).text = qsTr("Leave")

        PlainLabel {
            color: KodosiTheme.textPrimary
            text: qsTr("You lose access until the owner shares it with you again. The terminal keeps running.")
            width: 360
            wrapMode: Text.WordWrap
        }
    }
}
