pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property bool selected: Models.Projects.selectedProjectId.length > 0
    readonly property var projects: Models.Projects.projects
    readonly property var invitations: Models.Projects.invitations
    readonly property var terminals: {
        const byProject = {}
        for (const session of Models.Sessions.sessions) {
            if (session.projectId && session.status !== "closing")
                (byProject[session.projectId] = byProject[session.projectId] || []).push(session)
        }
        return byProject
    }
    readonly property int shared: Models.Sessions.sessions.filter(session => !!session.projectId && session.status !== "closing").length

    signal inspectSessionRequested(string sessionId)
    signal shareSessionRequested(string sessionId)
    signal newTerminalRequested(string projectId)

    function openCreate() {
        create.open();
    }

    objectName: "panel.projects"
    Accessible.id: objectName

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(420, parent.width - 48)
        visible: !root.accountReady
        title: qsTr("Work in projects")
        message: qsTr("A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.")
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
            objectName: "projects.signIn"
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
                    PlainLabel { text: qsTr("Projects"); font.pixelSize: KodosiTheme.fontLarge; font.weight: Font.DemiBold; font.letterSpacing: -0.6 }
                    RollingText {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        text: root.projects.length === 0 ? qsTr("A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.")
                            : qsTr("%n projects", "", root.projects.length) + " · "
                                + qsTr("%n shared terminals", "", root.shared)
                        color: KodosiTheme.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                    }
                }
                KButton {
                    variant: KButton.Primary
                    iconName: "plus"
                    text: qsTr("New project")
                    enabled: root.accountReady
                    objectName: "projects.create"
                    Accessible.id: objectName
                    onClicked: create.open()
                }
            }
            PlainLabel {
                Layout.fillWidth: true
                visible: Models.Projects.catalogTruncated
                text: qsTr("More projects exist. Use Go to… to find them.")
                color: KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontFootnote
                objectName: "panel.projects.truncated"
                Accessible.id: objectName
                Accessible.name: qsTr("Some projects are not shown")
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

                        ProjectSigil { key: invitation.modelData.projectId || ""; size: 40 }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            PlainLabel { Layout.fillWidth: true; text: invitation.modelData.projectName || ""; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold; elide: Text.ElideRight }
                            PlainLabel {
                                Layout.fillWidth: true
                                text: invitation.modelData.inviterName ? qsTr("%1 invited you").arg(invitation.modelData.inviterName) : qsTr("You are invited")
                                color: KodosiTheme.inkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                                elide: Text.ElideRight
                            }
                        }
                        KButton {
                            variant: KButton.Ghost
                            text: qsTr("Not now")
                            objectName: "panel.projectsView.decline." + invitation.modelData.id
                            Accessible.id: objectName
                            Accessible.name: qsTr("Decline invitation")
                            onClicked: Models.Projects.declineInvitation(invitation.modelData.id)
                        }
                        KButton {
                            variant: KButton.Primary
                            text: qsTr("Join")
                            objectName: "panel.projectsView.accept." + invitation.modelData.id
                            Accessible.id: objectName
                            onClicked: Models.Projects.acceptInvitation(invitation.modelData.id)
                        }
                    }
                }
            }
            CardGrid {
                id: grid
                Layout.fillWidth: true
                gap: 14

                Repeater {
                    model: root.projects

                    AbstractButton {
                        id: card

                        required property var modelData
                        readonly property bool unread: Models.Projects.unreadProjects.indexOf(modelData.id) >= 0
                        readonly property var sessions: root.terminals[modelData.id] || []
                        readonly property bool working: sessions.some(session => session.working === true)

                        width: grid.cardWidth
                        height: 132
                        hoverEnabled: true
                        activeFocusOnTab: true
                        objectName: "projects.project." + modelData.id
                        Accessible.id: objectName
                        Accessible.name: modelData.name
                        Accessible.description: unread ? qsTr("New messages") : ""
                        scale: pressed ? 0.98 : 1
                        transform: Translate { y: card.hovered ? -2 : 0; Behavior on y { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } } }

                        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                        background: Raised {
                            radius: KodosiTheme.radiusXl
                            fill: card.hovered ? KodosiTheme.lifted : KodosiTheme.raised
                            elevation: card.hovered ? 2 : 1
                        }
                        contentItem: Item {
                            ProjectSigil { x: 16; y: 16; key: card.modelData.id; size: 44 }
                            NewDot {
                                x: parent.width - width - 20
                                y: 20
                                visible: card.unread
                            }
                            Tag {
                                x: parent.width - width - 16
                                y: 16
                                visible: !card.unread && card.modelData.ownerUserId === Models.Account.userId
                                text: qsTr("Yours")
                            }
                            ColumnLayout {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                anchors.margins: 16
                                spacing: 6
                                PlainLabel { Layout.fillWidth: true; text: card.modelData.name; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold; elide: Text.ElideRight }
                                RowLayout {
                                    Layout.preferredHeight: 24
                                    spacing: 6

                                    PlainLabel {
                                        visible: card.sessions.length === 0
                                        text: qsTr("No terminals yet")
                                        color: KodosiTheme.inkFaint
                                        font.pixelSize: KodosiTheme.fontFootnote
                                    }
                                    Row {
                                        visible: card.sessions.length > 0
                                        spacing: -4

                                        Repeater {
                                            model: card.sessions.slice(0, 5)

                                            AgentMark {
                                                required property var modelData

                                                program: modelData.program || ""
                                                session: modelData
                                                size: 18
                                            }
                                        }
                                    }
                                    ShimmerText {
                                        visible: card.sessions.length > 0
                                        text: card.working ? qsTr("Working") : qsTr("%n terminals", "", card.sessions.length)
                                        color: KodosiTheme.inkMuted
                                        font.pixelSize: KodosiTheme.fontFootnote
                                    }
                                }
                            }
                        }
                        onClicked: Models.Projects.open(modelData.id)
                    }
                }
                AbstractButton {
                    id: slot
                    width: grid.cardWidth
                    height: 132
                    hoverEnabled: true
                    activeFocusOnTab: true
                    objectName: "projects.create.slot"
                    Accessible.id: objectName
                    Accessible.name: label.text
                    scale: pressed ? 0.98 : 1
                    background: DashedSlot { hovered: slot.hovered || slot.visualFocus }
                    contentItem: Item {
                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: 10
                            KIcon { Layout.alignment: Qt.AlignHCenter; Layout.preferredWidth: 18; Layout.preferredHeight: 18; name: "plus"; strokeWidth: 2.2 }
                            PlainLabel { id: label; text: root.projects.length === 0 ? qsTr("Make your first project") : qsTr("New project"); color: KodosiTheme.inkMuted; font.weight: Font.DemiBold }
                        }
                    }
                    onClicked: create.open()
                }
            }
            Item { Layout.preferredHeight: 30 }
        }
    }
    ProjectContent {
        anchors.fill: parent
        visible: root.accountReady && root.selected
        onInspectSessionRequested: id => root.inspectSessionRequested(id)
        onShareSessionRequested: id => root.shareSessionRequested(id)
        onNewTerminalRequested: projectId => root.newTerminalRequested(projectId)
    }
    KPopover {
        id: create
        objectName: "project.create.popup"
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
                ProjectSigil { key: name.text.trim().length > 0 ? name.text.trim() : "new-room"; size: 40 }
                PlainLabel { Layout.fillWidth: true; text: qsTr("New project"); font.pixelSize: KodosiTheme.fontTitle; font.weight: Font.DemiBold }
            }
            KTextField {
                id: name
                Layout.fillWidth: true
                placeholderText: qsTr("Project name")
                objectName: "project.create.name"
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
                    text: qsTr("Create project")
                    variant: KButton.Primary
                    working: Models.Projects.busy
                    enabled: name.text.trim().length > 0 && !Models.Projects.busy
                    objectName: "project.create"
                    Accessible.id: objectName
                    onClicked: Models.Projects.create(name.text)
                }
            }
        }
    }
    Connections {
        target: Models.Projects
        function onCreated(projectId) {
            name.clear();
            create.close();
        }
    }
}
