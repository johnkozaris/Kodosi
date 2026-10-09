pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    property var session: ({})
    property string sessionId: ""
    readonly property bool shareable: session.kind === "local" && session.isOwner === true
    readonly property string sharing: session.missionName ? qsTr("Shared with %1").arg(session.missionName)
        : (session.sharedWith || []).length === 1 ? qsTr("Shared with %1").arg(Identity.personName(session.sharedWith[0]))
        : (session.sharedWith || []).length > 1 ? qsTr("Shared with %1 people").arg(session.sharedWith.length)
        : session.isOwner === false ? qsTr("Shared by %1").arg(session.ownerName || qsTr("a friend"))
        : qsTr("Not shared")

    signal shareRequested(string sessionId)

    function openSession(id) {
        sessionId = id;
        refresh();
        open();
    }
    function refresh() {
        session = Models.Sessions.presentationForSession(sessionId);
    }

    focus: true
    modal: true
    objectName: "panel.sessionDetails"
    padding: 20
    parent: Overlay.overlay
    radius: KodosiTheme.radiusSheet
    elevation: 3
    width: Math.min(460, parent ? parent.width - 32 : 460)
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0

    Connections {
        function onModelReset() {
            root.refresh();
            if (!root.session.sessionId)
                root.close();
        }

        target: Models.Sessions
    }
    contentItem: ColumnLayout {
        spacing: 16

        RowLayout {
            spacing: 12

            AgentMark { program: root.session.program || ""; size: 40; working: root.session.working === true }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1

                TextInput {
                    id: name
                    Layout.fillWidth: true
                    text: root.session.name || ""
                    color: KodosiTheme.ink
                    font.pixelSize: KodosiTheme.fontTitle
                    font.weight: Font.DemiBold
                    selectionColor: KodosiTheme.accent
                    selectedTextColor: KodosiTheme.accentInk
                    selectByMouse: true
                    clip: true
                    readOnly: root.session.isOwner !== true
                    objectName: "panel.sessionDetails.name"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Terminal name")
                    Accessible.role: readOnly ? Accessible.StaticText : Accessible.EditableText
                    onEditingFinished: {
                        if (!readOnly && text.trim().length > 0 && text.trim() !== root.session.name)
                            Models.SessionActions.rename(root.sessionId, text);
                        else
                            text = Qt.binding(() => root.session.name || "");
                    }
                    Keys.onEscapePressed: { text = Qt.binding(() => root.session.name || ""); focus = false; }

                    Rectangle {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.bottom
                        height: 1.5
                        color: KodosiTheme.accent
                        visible: name.activeFocus
                    }
                }
                PlainLabel {
                    text: Identity.kindLabel(Identity.kind(root.session.program || ""))
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Close")
                glyph: "close"
                objectName: "panel.sessionDetails.close"

                onClicked: root.close()
            }
        }
        ListGroup {
            Layout.fillWidth: true

            ListRow {
                iconName: "laptop"
                tint: "#4f9bb0"
                title: root.session.hostLabel || ""
                subtitle: root.session.isOwner === false && root.session.ownerName ? root.session.ownerName : ""
            }
            ListRow {
                visible: !!root.session.displayDirectory
                iconName: "folder"
                tint: "#d9a441"
                title: root.session.folderName || ""
                subtitle: root.session.displayDirectory || ""

                KButton {
                    Accessible.id: objectName
                    compact: true
                    objectName: "panel.sessionDetails.open-project-folder"
                    text: qsTr("Open")
                    visible: root.session.kind === "local" && !!root.session.workingDirectory

                    onClicked: Models.DesktopFiles.openPath(root.session.workingDirectory)
                }
            }
            ListRow {
                iconName: "people"
                tint: "#607fcc"
                title: root.sharing
                subtitle: qsTr("Encrypted end to end")

                KButton {
                    Accessible.id: objectName
                    compact: true
                    variant: KButton.Tinted
                    objectName: "panel.sessionDetails.share"
                    text: qsTr("Share…")
                    visible: root.shareable

                    onClicked: {
                        root.close();
                        root.shareRequested(root.sessionId);
                    }
                }
                KButton {
                    Accessible.id: objectName
                    compact: true
                    variant: KButton.Ghost
                    objectName: "panel.sessionDetails.leave-shared-session"
                    text: qsTr("Leave…")
                    visible: root.session.isOwner === false && !root.session.missionId

                    onClicked: leaveConfirmation.open()
                }
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
            text: root.session.message || ""
            visible: !!root.session.message
            wrapMode: Text.WordWrap
        }
    }
    KDialog {
        id: leaveConfirmation

        destructive: true
        objectName: "panel.sessionDetails.leave-shared-session.confirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Leave %1?").arg(root.session.name || "")

        onAccepted: {
            Models.SessionActions.leave(root.sessionId);
            root.close();
        }
        onOpened: standardButton(Dialog.Ok).text = qsTr("Leave")

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: qsTr("The terminal keeps running. The owner can share it with you again.")
            width: 340
            wrapMode: Text.WordWrap
        }
    }
}
