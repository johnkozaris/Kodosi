pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property var presentation: Models.Projects.presentation
    readonly property var project: Models.Projects.selectedProject
    readonly property var content: Models.Projects.content
    readonly property var members: Models.Projects.members
    readonly property var sessionIds: Models.Projects.sessionIds
    readonly property int section: presentation.section || 0
    readonly property bool conversation: presentation.conversation !== false
    readonly property bool narrow: width < 720
    readonly property bool owner: project.ownerUserId === Models.Account.userId
    readonly property bool loaded: !!content.projectId
    readonly property var tasks: content.tasks || []
    readonly property int doneTasks: tasks.filter(task => task.closed).length
    readonly property string shownTerminal: presentation.terminal || ""
    readonly property bool terminalShown: visible && section === 0 && shownTerminal.length > 0
        && Models.DesktopState.stagedSessionIds.indexOf(shownTerminal) >= 0 && sessionIds.indexOf(shownTerminal) >= 0
    readonly property var fresh: presentation.fresh || []
    readonly property int unread: presentation.unread || 0
    readonly property real sheetWidth: Math.max(300, Math.min(540, presentation.conversationWidth || 372))
    readonly property var mine: Models.Sessions.sessions.filter(session => session.kind === "local" && session.isOwner === true
        && session.projectId !== Models.Projects.selectedProjectId)
    property bool renaming: false
    property Item selectedTab: null
    property rect tabRect: Qt.rect(0, 0, 0, 0)

    signal inspectSessionRequested(string sessionId)
    signal shareSessionRequested(string sessionId)
    signal newTerminalRequested(string projectId)

    function newTerminal() {
        root.newTerminalRequested(Models.Projects.selectedProjectId);
    }
    function placeTab() {
        if (!selectedTab || !projectStage.visible) {
            tabRect = Qt.rect(0, 0, 0, 0);
            return;
        }
        const place = selectedTab.mapToItem(projectStage, 0, 0, selectedTab.width, selectedTab.height);
        tabRect = Qt.rect(place.x, place.y, place.width, place.height);
    }
    function openTerminalMenu(sessionId) {
        terminalMenu.sessionId = sessionId
        terminalMenu.popup()
    }

    objectName: "project"
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

                ProjectSigil { key: Models.Projects.selectedProjectId; size: 32 }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 260
                    spacing: 1

                    PlainLabel {
                        Layout.fillWidth: true
                        visible: !root.renaming
                        text: root.project.name || ""
                        font.pixelSize: KodosiTheme.fontHeadline
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        objectName: "project.name"
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
                        objectName: "project.rename"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Project name")
                        onVisibleChanged: {
                            if (visible) {
                                text = root.project.name || "";
                                forceActiveFocus();
                                selectAll();
                            }
                        }
                        onAccepted: {
                            if (text.trim().length > 0 && text.trim() !== root.project.name)
                                Models.Projects.rename(text);
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
                        RollingText {
                            Layout.fillWidth: true
                            text: qsTr("%n people", "", root.members.length) + " · " + qsTr("%n terminals", "", root.sessionIds.length)
                            color: KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                            elide: Text.ElideRight
                        }
                    }
                }
                Item { Layout.fillWidth: true }
                SegmentedPill {
                    dock: true
                    identifier: "project.section"
                    currentValue: root.section
                    options: [
                        { value: 0, label: qsTr("Terminals"), icon: "terminal", count: root.sessionIds.length },
                        { value: 1, label: qsTr("Tasks"), count: root.tasks.length - root.doneTasks, progress: root.tasks.length > 0 ? root.doneTasks / root.tasks.length : 0,
                            fresh: root.fresh.indexOf(1) >= 0 },
                        { value: 2, label: qsTr("Repositories"), icon: "repository", count: (root.content.repositories || []).length,
                            fresh: root.fresh.indexOf(2) >= 0 }
                    ]
                    onActivated: value => {
                        Models.Projects.setPresentation("section", value);
                        if (root.narrow)
                            Models.Projects.setPresentation("conversation", false);
                    }
                }
                Item {
                    Layout.fillWidth: true

                    ReportReceived {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        shown: reportPerson.received
                    }
                }
                KIconButton {
                    glyph: "warning"
                    glyphColor: KodosiTheme.caution
                    visible: !!root.presentation.failure
                    Accessible.name: qsTr("Project action failed")
                    onClicked: failure.open()
                }
                AbstractButton {
                    id: peopleButton
                    implicitWidth: avatars.implicitWidth + 8
                    implicitHeight: 30
                    hoverEnabled: true
                    Accessible.name: qsTr("Project members")
                    objectName: "project.people"
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
                        transformOrigin: Popup.TopRight
                        width: 300
                        padding: 14
                        contentItem: ColumnLayout {
                            spacing: 10

                            PlainLabel { text: qsTr("In this project"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
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
                                        id: memberMore
                                        glyph: "more"
                                        size: 24
                                        visible: !member.self
                                        Accessible.name: qsTr("Options for %1").arg(member.fullName)
                                        objectName: "project.member.more." + member.modelData.userId
                                        Accessible.id: objectName
                                        onClicked: memberMenu.popup(memberMore, 0, memberMore.height + 4)

                                        KMenu {
                                            id: memberMenu
                                            KMenuItem {
                                                visible: root.owner
                                                text: qsTr("Remove from project")
                                                iconName: "close"
                                                destructive: true
                                                onTriggered: Models.Projects.removeMember(member.modelData.userId)
                                            }
                                            KMenuItem {
                                                text: qsTr("Block…")
                                                iconName: "hand"
                                                destructive: true
                                                onTriggered: { people.close(); blockPerson.ask(member.modelData.handle, member.fullName); }
                                            }
                                            KMenuItem {
                                                text: qsTr("Report…")
                                                iconName: "warning"
                                                destructive: true
                                                onTriggered: { people.close(); reportPerson.ask(member.modelData.handle, member.fullName); }
                                            }
                                        }
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
                                        enabled: !Models.Projects.busy
                                        objectName: "project.invite." + candidate.modelData.userId
                                        Accessible.id: objectName
                                        onClicked: Models.Projects.invite(candidate.modelData.userId)
                                    }
                                }
                            }
                            KButton {
                                Layout.fillWidth: true
                                visible: root.owner
                                variant: KButton.Ghost
                                iconName: "share"
                                text: qsTr("Add friends")
                                onClicked: { people.close(); Models.DesktopState.activeView = Models.DesktopState.People; }
                            }
                        }
                    }
                }
                KIconButton {
                    visible: root.conversation
                    glyph: "chat"
                    size: 30
                    active: true
                    Accessible.name: qsTr("Hide conversation")
                    onClicked: Models.Projects.setPresentation("conversation", false)
                    objectName: "project.conversation.hide"
                    Accessible.id: objectName
                }
                AbstractButton {
                    id: conversationKey

                    readonly property var last: (root.content.messages || []).slice(-1)[0] || null
                    readonly property var terminal: last && last.terminalId ? Models.Sessions.presentationForSession(last.terminalId) : ({})

                    visible: !root.conversation
                    implicitHeight: 30
                    implicitWidth: keyRow.implicitWidth + (root.unread > 0 ? 14 : 10)
                    hoverEnabled: true
                    scale: pressed ? 0.96 : 1
                    objectName: "project.conversation.toggle"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Show conversation")
                    Accessible.description: root.unread > 0 ? qsTr("%n new messages", "", root.unread) : ""
                    background: Raised {
                        radius: height / 2
                        fill: conversationKey.hovered ? KodosiTheme.lifted : KodosiTheme.raised
                        elevation: conversationKey.hovered ? 2 : 1
                    }
                    contentItem: Item {
                        RowLayout {
                            id: keyRow
                            x: 5
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6

                            AgentMark {
                                visible: !!conversationKey.terminal.id
                                program: conversationKey.terminal.program || ""
                                session: conversationKey.terminal
                                size: 20
                            }
                            PersonAvatar {
                                visible: !!conversationKey.last && !conversationKey.terminal.id
                                name: conversationKey.last ? conversationKey.last.agent || conversationKey.last.authorName || "" : ""
                                key: conversationKey.last ? conversationKey.last.authorId || "" : ""
                                isSelf: !!conversationKey.last && !conversationKey.last.agent && conversationKey.last.authorId === Models.Account.userId
                                size: 20
                            }
                            KIcon {
                                visible: !conversationKey.last
                                Layout.preferredWidth: 20
                                Layout.preferredHeight: 14
                                name: "chat"
                                color: KodosiTheme.inkMuted
                            }
                            RollingText {
                                visible: root.unread > 0
                                text: String(root.unread)
                                color: KodosiTheme.accentStrong
                                font.pixelSize: KodosiTheme.fontCaption
                                font.weight: Font.DemiBold
                            }
                        }
                    }
                    onClicked: Models.Projects.setPresentation("conversation", true)
                }
                KIconButton {
                    id: optionsButton
                    glyph: "more"
                    size: 30
                    open: options.visible
                    Accessible.name: qsTr("Project options")
                    objectName: "project.options"
                    Accessible.id: objectName
                    onClicked: options.visible ? options.close() : options.open()

                    KMenu {
                        id: options
                        x: optionsButton.width - width
                        y: optionsButton.height + 4
                        minimumWidth: 224
                        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                        transformOrigin: Popup.TopRight
                        KMenuItem { text: qsTr("Rename"); iconName: "pencil"; enabled: root.owner; onTriggered: root.renaming = true }
                        KMenuItem { text: qsTr("Copy agent instructions"); iconName: "agent"; onTriggered: Models.Projects.copyAgentInstructions() }
                        KMenuItem { text: qsTr("Refresh"); iconName: "refresh"; onTriggered: Models.Projects.act({ type: "read" }) }
                        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
                        KMenuItem { text: root.owner ? qsTr("Delete project…") : qsTr("Leave project…"); iconName: root.owner ? "trash" : "leave"; destructive: true; onTriggered: remove.open() }
                    }
                }
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                id: workArea
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                width: root.narrow ? parent.width : parent.width - (root.conversation ? root.sheetWidth + 8 : 0)
                visible: !root.narrow || !root.conversation

                TerminalDropArea {
                    anchors.fill: parent
                    anchors.margins: 6
                    z: 2
                    radius: KodosiTheme.radiusXl
                    label: qsTr("Share with %1").arg(root.project.name || "")
                    objectName: "project.drop"
                    onTerminalDropped: id => projectShare.ask(id, Models.Projects.selectedProjectId)
                }

                StackLayout {
                    anchors.fill: parent
                    currentIndex: root.section

                    ColumnLayout {
                        spacing: 0

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: 8
                            Layout.rightMargin: 8
                            Layout.bottomMargin: 8
                            visible: root.sessionIds.length > 0
                            spacing: 4

                            Item {
                                Layout.maximumWidth: workArea.width - 16
                                implicitWidth: tabs.implicitWidth + 6
                                implicitHeight: 34

                                Well { anchors.fill: parent; radius: 17 }
                                Flickable {
                                    anchors.fill: parent
                                    anchors.margins: 3
                                    contentWidth: tabs.implicitWidth
                                    clip: true
                                    boundsBehavior: Flickable.StopAtBounds
                                    onContentXChanged: Qt.callLater(root.placeTab)

                                    Row {
                                        id: tabs
                                        spacing: 2

                                        Repeater {
                                            model: root.sessionIds.length

                                            AbstractButton {
                                                id: tab

                                                required property int index
                                                readonly property string sessionId: root.sessionIds[index] || ""
                                                readonly property var session: Models.Sessions.sessions.find(item => item.id === sessionId) || ({})
                                                readonly property bool selected: root.shownTerminal === sessionId && terminal.active
                                                readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)

                                                height: 28
                                                implicitWidth: tabRow.implicitWidth + 20
                                                hoverEnabled: true
                                                activeFocusOnTab: true
                                                objectName: "project.terminal." + sessionId
                                                Accessible.id: objectName
                                                Accessible.name: session.name || qsTr("Terminal")
                                                Accessible.role: Accessible.PageTab
                                                Accessible.selected: selected
                                                ToolTip.text: session.folderName ? qsTr("%1 on %2").arg(session.folderName).arg(session.hostLabel || "") : session.hostLabel || ""
                                                ToolTip.visible: hovered && ToolTip.text.length > 0
                                                ToolTip.delay: Tip.delay

                                                onSelectedChanged: {
                                                    if (selected)
                                                        root.selectedTab = tab;
                                                    else if (root.selectedTab === tab)
                                                        root.selectedTab = null;
                                                    Qt.callLater(root.placeTab);
                                                }
                                                onXChanged: Qt.callLater(root.placeTab)
                                                onWidthChanged: Qt.callLater(root.placeTab)
                                                Component.onCompleted: {
                                                    if (selected) {
                                                        root.selectedTab = tab;
                                                        Qt.callLater(root.placeTab);
                                                    }
                                                }
                                                Component.onDestruction: {
                                                    if (root.selectedTab === tab)
                                                        root.selectedTab = null;
                                                }

                                                contentItem: Item {
                                                    RowLayout {
                                                        id: tabRow
                                                        x: 6
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        spacing: 7

                                                        AgentMark { program: tab.session.program || ""; size: 18; asleep: !tab.selected; session: tab.session }
                                                        PlainLabel {
                                                            text: tab.session.name || qsTr("Terminal")
                                                            color: tab.selected ? KodosiTheme.terminalInk : tab.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                                            font.pixelSize: KodosiTheme.fontFootnote
                                                            font.weight: Font.Medium
                                                        }
                                                        AvatarStack { visible: tab.viewers.length > 0; userIds: tab.viewers; size: 16; limit: 2; ring: tab.selected ? KodosiTheme.terminalBandSelected : KodosiTheme.well }
                                                        StatusSign { session: tab.session; unseen: Models.Sessions.attention.indexOf(tab.sessionId) >= 0 && !tab.selected; size: 12 }
                                                    }
                                                }
                                                background: Rectangle {
                                                    radius: height / 2
                                                    color: KodosiTheme.terminalBandSelected
                                                    visible: tab.selected
                                                }
                                                onClicked: Models.SessionActions.activateInProject(sessionId)

                                                TapHandler {
                                                    acceptedButtons: Qt.RightButton
                                                    enabled: tab.session.isOwner === true
                                                    onTapped: root.openTerminalMenu(tab.sessionId)
                                                }
                                            }
                                        }
                                        KIconButton {
                                            id: addButton
                                            glyph: "plus"
                                            size: 28
                                            Accessible.name: qsTr("Add terminal")
                                            objectName: "project.terminal.add"
                                            Accessible.id: objectName
                                            open: addMenu.visible
                                            onClicked: addMenu.popup(addButton, 0, addButton.height + 4)

                                            KMenu {
                                                id: addMenu
                                                KMenuItem { text: qsTr("New terminal"); iconName: "plus"; objectName: "project.terminal.add.new"; onTriggered: root.newTerminal() }
                                                MenuSeparator {
                                                    visible: root.mine.length > 0
                                                    height: visible ? implicitHeight : 0
                                                    contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
                                                }
                                                PlainLabel {
                                                    visible: root.mine.length > 0
                                                    height: visible ? implicitHeight : 0
                                                    leftPadding: 10
                                                    topPadding: 4
                                                    bottomPadding: 2
                                                    text: qsTr("Share one of mine")
                                                    color: KodosiTheme.inkFaint
                                                    font.pixelSize: KodosiTheme.fontCaption
                                                    font.weight: Font.Medium
                                                }
                                                Instantiator {
                                                    model: root.mine
                                                    onObjectAdded: (index, item) => addMenu.insertItem(index + 3, item)
                                                    onObjectRemoved: (index, item) => addMenu.removeItem(item)
                                                    delegate: KMenuItem {
                                                        required property var modelData
                                                        text: modelData.name
                                                        agent: modelData.program || ""
                                                        iconName: "terminal"
                                                        onTriggered: Models.SessionActions.attachProject(modelData.id, Models.Projects.selectedProjectId)
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Item { Layout.fillWidth: true }
                        }
                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true

                            CanvasStage {
                                id: projectStage

                                readonly property var shown: Models.Sessions.sessions.find(item => item.id === root.shownTerminal) || ({})

                                anchors.fill: parent
                                anchors.leftMargin: 6
                                anchors.rightMargin: 6
                                anchors.bottomMargin: 6
                                visible: root.terminalShown
                                always: true
                                radius: KodosiTheme.radiusXl
                                tab: root.tabRect
                                canvasSessionId: root.terminalShown && root.presentation.showsCanvas === true && !!shown.canvas
                                    && Models.Canvas.revision >= 0 && Models.Canvas.showsSomething(root.shownTerminal) ? root.shownTerminal : ""
                                wide: root.presentation.canvasWide === true
                                chosenWidth: root.presentation.canvasWidth || 0
                                onHideRequested: {
                                    Models.Canvas.closeCanvas(root.shownTerminal);
                                    Models.Projects.setPresentation("showsCanvas", false);
                                    Models.Projects.setPresentation("canvasWide", false);
                                }
                                onWideChosen: chosen => Models.Projects.setPresentation("canvasWide", chosen)
                                onWidthChosen: chosen => Models.Projects.setPresentation("canvasWidth", chosen)
                                onAsked: {
                                    const tile = terminal.item as TerminalTile;
                                    if (tile)
                                        tile.forceTerminalFocus();
                                }
                                onYChanged: Qt.callLater(root.placeTab)
                                onWidthChanged: Qt.callLater(root.placeTab)

                                Loader {
                                    id: terminal
                                    anchors.fill: parent
                                    active: root.terminalShown
                                    sourceComponent: TerminalTile {
                                        sessionId: root.shownTerminal
                                        projectEmbedded: true
                                        joined: true
                                        canvasOpen: projectStage.open
                                        interactionEnabled: root.visible && !Models.DesktopState.modalOpen && !projectStage.zoomed
                                        onInspectSessionRequested: id => root.inspectSessionRequested(id)
                                        onShareSessionRequested: id => root.shareSessionRequested(id)
                                        onCanvasRequested: section => {
                                            Models.Canvas.open(root.shownTerminal, section);
                                            Models.Projects.setPresentation("showsCanvas", true);
                                        }
                                        Component.onCompleted: Qt.callLater(forceTerminalFocus)
                                    }
                                }
                            }
                            EmptyState {
                                anchors.centerIn: parent
                                width: Math.min(360, parent.width - 40)
                                visible: !terminal.active && root.sessionIds.length === 0
                                title: qsTr("No terminals here yet")
                                message: qsTr("Add one. Your friends in the project get full control.")
                                art: Item {
                                    implicitWidth: 190
                                    implicitHeight: 96

                                    DashedSlot { anchors.fill: parent; radius: KodosiTheme.radiusLg }
                                    Row {
                                        x: 16
                                        y: 16
                                        spacing: 8

                                        KIcon { width: 18; height: 20; name: "prompt"; strokeWidth: 3; color: KodosiTheme.inkFaint }
                                        CursorBlock { width: 11; height: 20; blinks: true }
                                    }
                                }

                                KButton { text: qsTr("New terminal"); iconName: "plus"; variant: KButton.Primary; objectName: "project.terminal.new"; Accessible.id: objectName; onClicked: root.newTerminal() }
                                KButton {
                                    id: shareMine
                                    visible: root.mine.length > 0
                                    text: qsTr("Share one of mine")
                                    objectName: "project.terminal.share"
                                    Accessible.id: objectName
                                    onClicked: mineMenu.popup(shareMine, 0, shareMine.height + 4)

                                    KMenu {
                                        id: mineMenu

                                        Instantiator {
                                            model: root.mine
                                            onObjectAdded: (index, item) => mineMenu.insertItem(index, item)
                                            onObjectRemoved: (index, item) => mineMenu.removeItem(item)
                                            delegate: KMenuItem {
                                                required property var modelData
                                                text: modelData.name
                                                iconName: "terminal"
                                                onTriggered: Models.SessionActions.attachProject(modelData.id, Models.Projects.selectedProjectId)
                                            }
                                        }
                                    }
                                }
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
                                                objectName: "project.open." + modelData
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
                                                    StatusSign {
                                                        Layout.rightMargin: card.viewers.length > 0 ? 0 : 12
                                                        session: card.session
                                                        unseen: Models.Sessions.attention.indexOf(card.modelData) >= 0
                                                        size: 13
                                                    }
                                                    AvatarStack { Layout.rightMargin: 12; visible: card.viewers.length > 0; userIds: card.viewers; size: 20; limit: 2; ring: KodosiTheme.raised }
                                                }
                                                onClicked: Models.SessionActions.activateInProject(modelData)

                                                TapHandler {
                                                    acceptedButtons: Qt.RightButton
                                                    enabled: card.session.isOwner === true
                                                    onTapped: root.openTerminalMenu(card.modelData)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Item {
                        ProjectSkeleton { anchors.fill: parent; visible: !root.loaded && !root.presentation.failure }
                        ProjectTasks { anchors.fill: parent; visible: root.loaded }
                    }
                    Item {
                        ProjectSkeleton { anchors.fill: parent; visible: !root.loaded && !root.presentation.failure }
                        ProjectRepositories { anchors.fill: parent; visible: root.loaded }
                    }
                }
                EmptyState {
                    anchors.centerIn: parent
                    width: Math.min(340, parent.width - 40)
                    visible: !root.loaded && !!root.presentation.failure && root.section !== 0
                    title: qsTr("The project did not load")
                    message: root.presentation.failure || ""

                    KButton { text: qsTr("Try again"); iconName: "refresh"; variant: KButton.Primary; onClicked: Models.Projects.act({ type: "read" }) }
                }
            }
            Item {
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                anchors.right: parent.right
                anchors.rightMargin: root.conversation ? 6 : -(width + 20)
                width: root.narrow ? parent.width - 12 : root.sheetWidth
                visible: anchors.rightMargin > -(width + 20)

                Behavior on anchors.rightMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }

                Raised { anchors.fill: parent; radius: KodosiTheme.radiusXl; fill: KodosiTheme.sheet; elevation: 2 }
                ProjectConversation {
                    anchors.fill: parent
                    onBlockAsked: (handle, name) => blockPerson.ask(handle, name)
                    onReportAsked: (handle, name) => reportPerson.ask(handle, name)
                    onTerminalRequested: id => {
                        Models.Projects.setPresentation("section", 0);
                        Models.SessionActions.activateInProject(id);
                        if (root.narrow)
                            Models.Projects.setPresentation("conversation", false);
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
                            Models.Projects.setPresentation("conversationWidth", Math.max(300, Math.min(540, startWidth + startX - mapToItem(root, mouse.x, 0).x)));
                    }
                }
            }
        }
    }
    ProjectShareDialog {
        id: projectShare
        onShared: Models.SessionActions.activateInProject(projectShare.sessionId)
    }
    KMenu {
        id: terminalMenu

        property string sessionId: ""

        KMenuItem {
            text: qsTr("Remove from project")
            iconName: "leave"
            objectName: "project.terminal.remove"
            onTriggered: Models.SessionActions.attachProject(terminalMenu.sessionId, "")
        }
    }
    KDialog {
        id: remove
        destructive: true
        title: root.owner ? qsTr("Delete %1?").arg(root.project.name || "") : qsTr("Leave %1?").arg(root.project.name || "")
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: standardButton(Dialog.Ok).text = root.owner ? qsTr("Delete") : qsTr("Leave")
        onAccepted: root.owner ? Models.Projects.remove() : Models.Projects.leave()

        PlainLabel {
            width: 340
            color: KodosiTheme.inkMuted
            wrapMode: Text.WordWrap
            text: root.owner ? qsTr("Kodosi deletes the conversation and tasks for everyone. The terminals keep running.") : qsTr("You lose its terminals, conversation and tasks.")
        }
    }
    KPopover {
        id: failure
        x: Math.max(0, root.width - width - 18)
        y: 52
        transformOrigin: Popup.TopRight
        width: 320
        padding: 16
        contentItem: ColumnLayout {
            spacing: 12
            KReadOnlyText { Layout.fillWidth: true; text: root.presentation.failure || "" }
            KButton { text: qsTr("Try again"); compact: true; onClicked: { Models.Projects.act({ type: "read" }); failure.close(); } }
        }
    }
    BlockPersonDialog { id: blockPerson }
    ReportPersonDialog { id: reportPerson }
}
