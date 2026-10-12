pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property var presentation: Models.Missions.presentation
    readonly property var mission: Models.Missions.selectedMission
    readonly property var room: Models.Missions.room
    readonly property var members: Models.Missions.members
    readonly property var sessionIds: Models.Missions.sessionIds
    readonly property int canvas: presentation.canvas || 0
    readonly property bool conversation: presentation.conversation !== false
    readonly property bool narrow: width < 720
    readonly property bool owner: mission.ownerUserId === Models.Account.userId
    readonly property bool loaded: !!room.roomId
    readonly property var tasks: room.tasks || []
    readonly property int doneTasks: tasks.filter(task => task.closed).length
    readonly property string shownTerminal: presentation.terminal || ""
    readonly property real sheetWidth: Math.max(300, Math.min(540, presentation.conversationWidth || 372))
    property bool renaming: false

    signal inspectSessionRequested(string sessionId)
    signal shareSessionRequested(string sessionId)

    function newTerminal() {
        Models.SessionActions.createInRoom(Models.Missions.selectedMissionId, Models.DesktopSettings.effectiveWorkingDirectory);
    }

    objectName: "room"
    Accessible.id: objectName

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: KodosiTheme.headerHeight

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 10
                spacing: 10

                RoomSigil { key: Models.Missions.selectedMissionId; size: 32 }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 260
                    spacing: 1

                    PlainLabel {
                        Layout.fillWidth: true
                        visible: !root.renaming
                        text: root.mission.name || ""
                        font.pixelSize: KodosiTheme.fontHeadline
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        objectName: "room.name"
                        Accessible.id: objectName

                        TapHandler {
                            enabled: root.owner
                            onDoubleTapped: root.renaming = true
                        }
                    }
                    KTextField {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 24
                        visible: root.renaming
                        topPadding: 2
                        bottomPadding: 2
                        leftPadding: 8
                        font.weight: Font.DemiBold
                        objectName: "room.rename"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Room name")
                        onVisibleChanged: {
                            if (visible) {
                                text = root.mission.name || "";
                                forceActiveFocus();
                                selectAll();
                            }
                        }
                        onAccepted: {
                            if (text.trim().length > 0 && text.trim() !== root.mission.name)
                                Models.Missions.rename(text);
                            root.renaming = false;
                        }
                        onActiveFocusChanged: {
                            if (!activeFocus)
                                root.renaming = false;
                        }
                        Keys.onEscapePressed: root.renaming = false
                    }
                    RowLayout {
                        spacing: 5
                        visible: !root.renaming
                        KIcon { Layout.preferredWidth: 10; Layout.preferredHeight: 10; name: "lock"; strokeWidth: 2.4; color: KodosiTheme.inkFaint }
                        PlainLabel {
                            text: Identity.count(root.members.length, qsTr("1 person"), qsTr("%1 people")) + " · " + Identity.count(root.sessionIds.length, qsTr("1 terminal"), qsTr("%1 terminals"))
                            color: KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                        }
                    }
                }
                Item { Layout.fillWidth: true }
                SectionDock {
                    identifier: "room.canvas"
                    currentIndex: root.canvas
                    items: [
                        { label: qsTr("Terminals"), icon: "terminal", count: root.sessionIds.length },
                        { label: qsTr("Tasks"), icon: "tasks", count: root.tasks.length - root.doneTasks, progress: root.tasks.length > 0 ? root.doneTasks / root.tasks.length : 0 },
                        { label: qsTr("Repositories"), icon: "repository", count: (root.room.repositories || []).length }
                    ]
                    onActivated: index => {
                        Models.Missions.setPresentation("canvas", index);
                        if (root.narrow)
                            Models.Missions.setPresentation("conversation", false);
                    }
                }
                Item { Layout.fillWidth: true }
                KIconButton {
                    glyph: "warning"
                    glyphColor: KodosiTheme.caution
                    visible: !!root.presentation.failure
                    Accessible.name: qsTr("Room action failed")
                    onClicked: failure.open()
                }
                AbstractButton {
                    id: peopleButton
                    implicitWidth: avatars.implicitWidth + 8
                    implicitHeight: 30
                    hoverEnabled: true
                    Accessible.name: qsTr("Room members")
                    objectName: "room.people"
                    Accessible.id: objectName
                    contentItem: Item {
                        AvatarStack {
                            id: avatars
                            anchors.centerIn: parent
                            userIds: root.members.map(member => member.userId)
                            size: 24
                            ring: KodosiTheme.surface
                        }
                    }
                    background: Rectangle { radius: 15; color: KodosiTheme.alpha(KodosiTheme.ink, peopleButton.hovered ? 0.07 : 0) }
                    onClicked: people.open()

                    KPopover {
                        id: people
                        focus: true
                        x: peopleButton.width - width
                        y: peopleButton.height + 8
                        width: 300
                        padding: 14
                        contentItem: ColumnLayout {
                            spacing: 10

                            PlainLabel { text: qsTr("In this room"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
                            Repeater {
                                model: root.members
                                delegate: RowLayout {
                                    id: member
                                    required property var modelData
                                    readonly property bool self: modelData.userId === Models.Account.userId
                                    readonly property string fullName: modelData.displayName || modelData.handle
                                    Layout.fillWidth: true
                                    spacing: 10

                                    PersonAvatar { name: member.fullName; key: member.modelData.userId; size: 28; isSelf: member.self }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 0
                                        PlainLabel { Layout.fillWidth: true; text: member.self ? qsTr("%1 (you)").arg(member.fullName) : member.fullName; font.weight: Font.Medium; elide: Text.ElideRight }
                                        PlainLabel { text: "@" + member.modelData.handle; color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption }
                                    }
                                    Tag { visible: member.modelData.isOwner === true; text: qsTr("Owner") }
                                    KIconButton {
                                        glyph: "close"
                                        size: 24
                                        destructive: true
                                        visible: root.owner && !member.modelData.isOwner
                                        Accessible.name: qsTr("Remove %1 from the room").arg(member.fullName)
                                        onClicked: Models.Missions.removeMember(member.modelData.userId)
                                    }
                                }
                            }
                            PlainLabel { visible: root.owner; Layout.topMargin: 4; text: qsTr("Invite"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
                            Repeater {
                                model: root.owner ? Models.People.friends.filter(friend => !root.members.some(member => member.userId === friend.userId)) : []
                                delegate: RowLayout {
                                    id: candidate
                                    required property var modelData
                                    readonly property string fullName: modelData.displayName || modelData.handle
                                    Layout.fillWidth: true
                                    spacing: 10

                                    PersonAvatar { name: candidate.fullName; key: candidate.modelData.userId; size: 28 }
                                    PlainLabel { Layout.fillWidth: true; text: candidate.fullName; font.weight: Font.Medium; elide: Text.ElideRight }
                                    KButton {
                                        compact: true
                                        variant: KButton.Tinted
                                        text: qsTr("Invite")
                                        enabled: !Models.Missions.busy
                                        objectName: "room.invite." + candidate.modelData.userId
                                        Accessible.id: objectName
                                        onClicked: Models.Missions.invite(candidate.modelData.userId)
                                    }
                                }
                            }
                            KButton {
                                Layout.fillWidth: true
                                visible: root.owner
                                variant: KButton.Ghost
                                iconName: "share"
                                text: qsTr("Add friends in People")
                                onClicked: { people.close(); Models.DesktopState.activeView = Models.DesktopState.People; }
                            }
                        }
                    }
                }
                KIconButton {
                    glyph: "chat"
                    size: 30
                    active: root.conversation
                    Accessible.name: root.conversation ? qsTr("Hide conversation") : qsTr("Show conversation")
                    onClicked: Models.Missions.setPresentation("conversation", !root.conversation)
                    objectName: "room.conversation.toggle"
                    Accessible.id: objectName
                }
                KIconButton {
                    id: optionsButton
                    glyph: "more"
                    size: 30
                    Accessible.name: qsTr("Room options")
                    objectName: "room.options"
                    Accessible.id: objectName
                    onClicked: options.popup(optionsButton, 0, optionsButton.height + 4)

                    KMenu {
                        id: options
                        KMenuItem { text: qsTr("Rename"); iconName: "pencil"; enabled: root.owner; onTriggered: root.renaming = true }
                        KMenuItem { text: qsTr("Copy agent instructions"); iconName: "agent"; onTriggered: Models.Missions.copyAgentInstructions() }
                        KMenuItem { text: qsTr("Refresh"); iconName: "refresh"; onTriggered: Models.Missions.roomAction({ type: "read" }) }
                        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.alpha(KodosiTheme.hairline, 0.7) } }
                        KMenuItem { text: root.owner ? qsTr("Delete room…") : qsTr("Leave room…"); iconName: "close"; destructive: true; onTriggered: remove.open() }
                    }
                }
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                id: canvasArea
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                width: root.narrow ? parent.width : parent.width - (root.conversation ? root.sheetWidth + 8 : 0)
                visible: !root.narrow || !root.conversation

                StackLayout {
                    anchors.fill: parent
                    currentIndex: root.canvas

                    ColumnLayout {
                        spacing: 0

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: 8
                            Layout.rightMargin: 8
                            Layout.bottomMargin: 6
                            visible: root.sessionIds.length > 0
                            spacing: 4

                            Item {
                                Layout.maximumWidth: canvasArea.width - 60
                                implicitWidth: tabs.implicitWidth + 6
                                implicitHeight: 34

                                Well { anchors.fill: parent; radius: 17 }
                                Flickable {
                                    anchors.fill: parent
                                    anchors.margins: 3
                                    contentWidth: tabs.implicitWidth
                                    clip: true
                                    boundsBehavior: Flickable.StopAtBounds

                                    Row {
                                        id: tabs
                                        spacing: 2

                                        Repeater {
                                            model: root.sessionIds.length

                                            AbstractButton {
                                                id: tab

                                                required property int index
                                                readonly property string sessionId: root.sessionIds[index] || ""
                                                readonly property var session: Models.Sessions.sessions.find(entry => entry.id === sessionId) || ({})
                                                readonly property bool selected: root.shownTerminal === sessionId && terminal.active
                                                readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)

                                                height: 28
                                                implicitWidth: tabRow.implicitWidth + 20
                                                hoverEnabled: true
                                                activeFocusOnTab: true
                                                objectName: "room.terminal." + sessionId
                                                Accessible.id: objectName
                                                Accessible.name: session.name || qsTr("Terminal")
                                                Accessible.role: Accessible.PageTab
                                                Accessible.selected: selected

                                                contentItem: Item {
                                                    RowLayout {
                                                        id: tabRow
                                                        x: 8
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        spacing: 7

                                                        AgentMark { program: tab.session.program || ""; size: 18; asleep: !tab.selected; session: tab.session }
                                                        PlainLabel {
                                                            text: tab.session.name || qsTr("Terminal")
                                                            color: tab.selected || tab.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                                            font.pixelSize: KodosiTheme.fontFootnote
                                                            font.weight: tab.selected ? Font.DemiBold : Font.Medium
                                                        }
                                                        AvatarStack { visible: tab.viewers.length > 0; userIds: tab.viewers; size: 16; limit: 2; ring: tab.selected ? KodosiTheme.raised : KodosiTheme.well }
                                                        StatusSign { session: tab.session; unseen: Models.Sessions.attention.indexOf(tab.sessionId) >= 0 && !tab.selected; size: 12 }
                                                    }
                                                }
                                                background: Item {
                                                    Raised { anchors.fill: parent; radius: 14; visible: tab.selected }
                                                }
                                                onClicked: {
                                                    Models.Sessions.clearAttention(sessionId);
                                                    Models.SessionActions.activateInRoom(sessionId);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            KIconButton {
                                id: addButton
                                glyph: "plus"
                                size: 30
                                Accessible.name: qsTr("Add terminal")
                                objectName: "room.terminal.add"
                                Accessible.id: objectName
                                onClicked: addMenu.popup(addButton, 0, addButton.height + 4)

                                KMenu {
                                    id: addMenu
                                    KMenuItem { text: qsTr("New terminal"); iconName: "plus"; onTriggered: root.newTerminal() }
                                    Instantiator {
                                        model: Models.Sessions.sessions.filter(session => session.kind === "local" && session.isOwner === true && session.missionId !== Models.Missions.selectedMissionId)
                                        onObjectAdded: (index, item) => addMenu.insertItem(index + 1, item)
                                        onObjectRemoved: (index, item) => addMenu.removeItem(item)
                                        delegate: KMenuItem {
                                            required property var modelData
                                            text: qsTr("Share %1").arg(modelData.name)
                                            iconName: "terminal"
                                            onTriggered: Models.SessionActions.attachMission(modelData.id, Models.Missions.selectedMissionId)
                                        }
                                    }
                                }
                            }
                            Item { Layout.fillWidth: true }
                        }
                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true

                            Loader {
                                id: terminal
                                anchors.fill: parent
                                anchors.leftMargin: 6
                                anchors.rightMargin: 6
                                anchors.bottomMargin: 6
                                active: root.visible && root.canvas === 0 && root.shownTerminal.length > 0 && Models.DesktopState.stagedSessionIds.indexOf(root.shownTerminal) >= 0 && root.sessionIds.indexOf(root.shownTerminal) >= 0
                                sourceComponent: TerminalTile {
                                    sessionId: root.shownTerminal
                                    roomEmbedded: true
                                    interactionEnabled: root.visible && !Models.DesktopState.modalOpen
                                    onInspectSessionRequested: (id, name) => root.inspectSessionRequested(id)
                                    onShareSessionRequested: (id, name) => root.shareSessionRequested(id)
                                    Component.onCompleted: Qt.callLater(forceTerminalFocus)
                                }
                            }
                            EmptyState {
                                anchors.centerIn: parent
                                width: Math.min(360, parent.width - 40)
                                visible: !terminal.active && root.sessionIds.length === 0
                                title: qsTr("No terminals here yet")
                                message: qsTr("Add one. Everyone in the room gets full control.")
                                art: AgentMark { size: 52; asleep: true }

                                KButton { text: qsTr("New terminal"); iconName: "plus"; variant: KButton.Primary; objectName: "room.terminal.new"; Accessible.id: objectName; onClicked: root.newTerminal() }
                            }
                            KScrollView {
                                id: openGrid
                                anchors.fill: parent
                                visible: !terminal.active && root.sessionIds.length > 0
                                contentWidth: availableWidth

                                ColumnLayout {
                                    x: 22
                                    width: openGrid.availableWidth - 44
                                    spacing: 14

                                    PlainLabel { Layout.topMargin: 14; text: qsTr("Open a terminal"); font.pixelSize: KodosiTheme.fontTitle; font.weight: Font.DemiBold }
                                    CardGrid {
                                        id: grid
                                        Layout.fillWidth: true
                                        minimum: 240

                                        Repeater {
                                            model: root.sessionIds

                                            AbstractButton {
                                                id: card

                                                required property string modelData
                                                readonly property var session: Models.Sessions.sessions.find(entry => entry.id === modelData) || ({})
                                                readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)

                                                width: grid.cardWidth
                                                height: 62
                                                hoverEnabled: true
                                                activeFocusOnTab: true
                                                Accessible.name: session.name || ""
                                                objectName: "room.open." + modelData
                                                Accessible.id: objectName
                                                scale: pressed ? 0.98 : 1
                                                background: Raised { radius: KodosiTheme.radiusLg; fill: card.hovered ? KodosiTheme.lifted : KodosiTheme.raised; elevation: card.hovered ? 2 : 1 }
                                                contentItem: RowLayout {
                                                    spacing: 12
                                                    AgentMark { Layout.leftMargin: 12; program: card.session.program || ""; size: 32; session: card.session }
                                                    ColumnLayout {
                                                        Layout.fillWidth: true
                                                        spacing: 2
                                                        PlainLabel { Layout.fillWidth: true; text: card.session.name || ""; font.weight: Font.DemiBold; elide: Text.ElideRight }
                                                        RowLayout {
                                                            spacing: 5
                                                            KIcon { Layout.preferredWidth: 11; Layout.preferredHeight: 11; name: "laptop"; color: KodosiTheme.inkFaint }
                                                            ActivityLine {
                                                                Layout.fillWidth: true
                                                                session: card.session
                                                                words: card.session.activity || card.session.hostLabel || ""
                                                            }
                                                        }
                                                    }
                                                    AvatarStack { Layout.rightMargin: 12; visible: card.viewers.length > 0; userIds: card.viewers; size: 20; limit: 2; ring: KodosiTheme.raised }
                                                }
                                                onClicked: Models.SessionActions.activateInRoom(modelData)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Item {
                        RoomSkeleton { anchors.fill: parent; visible: !root.loaded && !root.presentation.failure }
                        RoomTasks { anchors.fill: parent; visible: root.loaded }
                    }
                    Item {
                        RoomSkeleton { anchors.fill: parent; visible: !root.loaded && !root.presentation.failure }
                        RoomRepositories { anchors.fill: parent; visible: root.loaded }
                    }
                }
                EmptyState {
                    anchors.centerIn: parent
                    width: Math.min(340, parent.width - 40)
                    visible: !root.loaded && !!root.presentation.failure && root.canvas !== 0
                    title: qsTr("The room did not load")
                    message: root.presentation.failure || ""

                    KButton { text: qsTr("Try again"); iconName: "refresh"; variant: KButton.Primary; onClicked: Models.Missions.roomAction({ type: "read" }) }
                }
            }
            Item {
                id: sheet
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                anchors.right: parent.right
                anchors.rightMargin: root.conversation ? 6 : -(width + 20)
                width: root.narrow ? parent.width - 12 : root.sheetWidth
                visible: anchors.rightMargin > -(width + 20)

                Behavior on anchors.rightMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }

                Raised { anchors.fill: parent; radius: KodosiTheme.radiusXl; fill: KodosiTheme.raised; elevation: 2 }
                RoomConversation {
                    anchors.fill: parent
                    onTerminalRequested: id => {
                        Models.Missions.setPresentation("canvas", 0);
                        Models.SessionActions.activateInRoom(id);
                        if (root.narrow)
                            Models.Missions.setPresentation("conversation", false);
                    }
                }
                MouseArea {
                    visible: !root.narrow
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left
                    anchors.leftMargin: -5
                    width: 10
                    cursorShape: Qt.SizeHorCursor
                    preventStealing: true
                    property real startX: 0
                    property real startWidth: 0
                    Accessible.role: Accessible.Slider
                    Accessible.name: qsTr("Resize conversation")
                    onPressed: mouse => { startX = mapToItem(root, mouse.x, 0).x; startWidth = root.sheetWidth; }
                    onPositionChanged: mouse => {
                        if (pressed)
                            Models.Missions.setPresentation("conversationWidth", Math.max(300, Math.min(540, startWidth + startX - mapToItem(root, mouse.x, 0).x)));
                    }
                }
            }
        }
    }
    KDialog {
        id: remove
        destructive: true
        title: root.owner ? qsTr("Delete %1?").arg(root.mission.name || "") : qsTr("Leave %1?").arg(root.mission.name || "")
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: standardButton(Dialog.Ok).text = root.owner ? qsTr("Delete") : qsTr("Leave")
        onAccepted: root.owner ? Models.Missions.remove() : Models.Missions.leave()

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            wrapMode: Text.WordWrap
            text: root.owner ? qsTr("The conversation and tasks go away for everyone. The terminals keep running.") : qsTr("You lose its terminals, conversation and tasks.")
        }
    }
    KPopover {
        id: failure
        x: Math.max(0, root.width - width - 18)
        y: 52
        width: 320
        padding: 16
        contentItem: ColumnLayout {
            spacing: 12
            KReadOnlyText { Layout.fillWidth: true; text: root.presentation.failure || "" }
            KButton { text: qsTr("Try again"); compact: true; onClicked: { Models.Missions.roomAction({ type: "read" }); failure.close(); } }
        }
    }
}
