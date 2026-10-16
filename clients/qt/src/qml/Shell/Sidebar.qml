pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property bool collapsed: !Models.DesktopState.sidebarOpen
    readonly property var groups: Models.Sessions.folderGroups
    readonly property var rooms: Models.Missions.missions
    readonly property var invitations: Models.Missions.invitations
    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property var roomWork: {
        const work = {}
        for (const session of Models.Sessions.sessions) {
            if (!session.missionId)
                continue
            const room = work[session.missionId] || (work[session.missionId] = { asks: false, working: false })
            room.asks = room.asks || session.asks === true
            room.working = room.working || session.working === true
        }
        return work
    }
    readonly property string selection: {
        switch (Models.DesktopState.activeView) {
        case Models.DesktopState.Sessions: return Models.DesktopState.selectedSessionId.length > 0 ? "terminal:" + Models.DesktopState.selectedSessionId : ""
        case Models.DesktopState.Missions: return Models.Missions.selectedMissionId.length > 0 ? "room:" + Models.Missions.selectedMissionId : "rooms"
        case Models.DesktopState.People: return "people"
        default: return "settings"
        }
    }
    property bool titleBar: false
    property string renamingId: ""
    property Item pillTarget: null
    property var collapsedFolders: ({})

    signal newTerminalRequested(string folder)
    signal detailsRequested(string sessionId)
    signal shareRequested(string sessionId)
    signal resumeRequested
    signal paletteRequested
    signal newRoomRequested

    function signIn() {
        Models.DesktopState.activeView = Models.DesktopState.Settings
        if (!Models.Account.signedIn)
            Models.Account.login()
    }
    function showRooms() {
        Models.DesktopState.activeView = Models.DesktopState.Missions
        Models.Missions.open("")
    }
    function openRoom(id) {
        Models.DesktopState.activeView = Models.DesktopState.Missions
        Models.Missions.open(id)
    }
    function openTerminal(id) {
        Models.Sessions.clearAttention(id)
        Models.SessionActions.activate(id)
    }
    function syncPill() {
        if (!pillTarget || collapsed) {
            pill.visible = false
            return
        }
        const origin = pillTarget.mapToItem(content, 0, 0)
        pill.x = content.x + origin.x
        pill.y = content.y + origin.y
        pill.width = pillTarget.width
        pill.height = pillTarget.height
        pill.visible = true
    }
    function claimPill(item, selected) {
        if (selected)
            pillTarget = item
        else if (pillTarget === item)
            pillTarget = null
    }

    objectName: "sidebar"
    Accessible.id: objectName
    Accessible.name: qsTr("Rooms and terminals")
    Accessible.role: Accessible.Pane
    implicitWidth: collapsed ? KodosiTheme.railWidth : KodosiTheme.sidebarWidth

    onPillTargetChanged: Qt.callLater(syncPill)
    onGroupsChanged: Qt.callLater(syncPill)
    onRoomsChanged: Qt.callLater(syncPill)
    onInvitationsChanged: Qt.callLater(syncPill)
    onCollapsedChanged: Qt.callLater(syncPill)

    component SectionHeader: Item {
        id: header

        property string title: ""
        property string addName: ""
        property string identifier: ""
        property bool selected: false

        signal clicked
        signal addClicked

        width: parent ? parent.width : 0
        height: 28

        AbstractButton {
            id: titleButton
            anchors.left: parent.left
            anchors.right: add.left
            anchors.rightMargin: 4
            height: parent.height
            hoverEnabled: true
            Accessible.name: header.title
            objectName: header.identifier
            Accessible.id: objectName
            contentItem: PlainLabel {
                leftPadding: 10
                text: header.title
                color: header.selected ? KodosiTheme.accentStrong : titleButton.hovered ? KodosiTheme.inkMuted : KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
                font.weight: Font.DemiBold
                verticalAlignment: Text.AlignVCenter
            }
            onClicked: header.clicked()
        }
        KIconButton {
            id: add
            anchors.right: parent.right
            anchors.rightMargin: 2
            anchors.verticalCenter: parent.verticalCenter
            size: 22
            glyph: "plus"
            objectName: header.identifier + ".add"
            Accessible.id: objectName
            Accessible.name: header.addName
            ToolTip.visible: hovered
            ToolTip.text: header.addName
            ToolTip.delay: 600
            onClicked: header.addClicked()
        }
    }

    component NavRow: AbstractButton {
        id: nav

        property string title: ""
        property string iconName: ""
        property int badge: 0
        property bool selected: false

        width: parent ? parent.width : 0
        height: 34
        hoverEnabled: true
        activeFocusOnTab: true
        Accessible.name: title
        Accessible.id: objectName
        Accessible.selected: selected

        contentItem: RowLayout {
            spacing: 9

            Item {
                Layout.preferredWidth: 22
                Layout.preferredHeight: 22
                Layout.leftMargin: 8
                KIcon {
                    anchors.centerIn: parent
                    width: 15
                    height: 15
                    name: nav.iconName
                    color: nav.selected ? KodosiTheme.accentStrong : nav.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                }
            }
            PlainLabel {
                Layout.fillWidth: true
                text: nav.title
                color: nav.selected || nav.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                font.weight: nav.selected ? Font.DemiBold : Font.Medium
                elide: Text.ElideRight
            }
            CountBadge {
                Layout.rightMargin: 10
                count: nav.badge
            }
        }
        background: Item {
            Raised {
                anchors.fill: parent
                radius: KodosiTheme.radiusMd
                visible: nav.selected
            }
            Rectangle {
                anchors.fill: parent
                radius: KodosiTheme.radiusMd
                color: KodosiTheme.alpha(KodosiTheme.ink, !nav.selected && (nav.hovered || nav.visualFocus) ? 0.06 : 0)
            }
        }
    }

    component RailItem: AbstractButton {
        id: item

        property bool selected: false

        anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
        width: 40
        height: 40
        hoverEnabled: true
        scale: pressed ? 0.94 : 1
        Accessible.selected: selected
        ToolTip.visible: hovered
        ToolTip.text: Accessible.name
        ToolTip.delay: 600

        background: Item {
            Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; visible: item.selected }
            Rectangle {
                anchors.fill: parent
                radius: KodosiTheme.radiusLg
                color: KodosiTheme.alpha(KodosiTheme.ink, !item.selected && (item.hovered || item.visualFocus) ? 0.06 : 0)
            }
        }
    }

    ColumnLayout {
        id: rail
        anchors.fill: parent
        visible: root.collapsed
        spacing: 10

        Column {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: root.titleBar ? (KodosiTheme.titleBarHeight - 28) / 2 + 3 : 10
            spacing: 10

            KIconButton {
                anchors.horizontalCenter: parent.horizontalCenter
                glyph: "sidebar"
                objectName: "sidebar.toggle"
                Accessible.id: objectName
                Accessible.name: qsTr("Show sidebar")
                onClicked: Models.DesktopState.sidebarOpen = true
            }
            AbstractButton {
                id: railNew
                anchors.horizontalCenter: parent.horizontalCenter
                width: 34
                height: 34
                hoverEnabled: true
                scale: pressed ? 0.94 : 1
                objectName: "rail.newTerminal"
                Accessible.id: objectName
                Accessible.name: qsTr("New terminal")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("New terminal (Ctrl+Shift+N)")
                ToolTip.delay: 600
                background: Bezel {
                    radius: 17
                    fill: railNew.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                    pressed: railNew.pressed
                }
                contentItem: Item {
                    KIcon { anchors.centerIn: parent; width: 14; height: 14; name: "plus"; color: KodosiTheme.accentInk; strokeWidth: 2.6 }
                }
                onClicked: root.newTerminalRequested("")
            }
        }
        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: railColumn.implicitHeight + 12
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: railColumn
                y: 6
                width: parent.width
                spacing: 6

                Repeater {
                    model: root.rooms.length

                    RailItem {
                        id: railRoom

                        required property int index
                        readonly property var entry: root.rooms[index] || ({})

                        selected: root.selection === "room:" + entry.id
                        Accessible.name: entry.name || ""
                        contentItem: Item {
                            RoomSigil {
                                id: railSigil
                                anchors.centerIn: parent
                                key: railRoom.entry.id || ""
                                size: 26
                            }
                            NewDot {
                                x: railSigil.x + railSigil.width - width / 2 - 1
                                y: railSigil.y - height / 2 + 1
                                visible: Models.Missions.unreadRooms.indexOf(railRoom.entry.id) >= 0
                                ring: KodosiTheme.ground
                            }
                        }
                        onClicked: root.openRoom(entry.id)
                    }
                }
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 18
                    height: 1
                    radius: 0.5
                    color: KodosiTheme.alpha(KodosiTheme.hairline, 0.7)
                    visible: root.rooms.length > 0 && Models.Sessions.count > 0
                }
                Repeater {
                    model: Models.Sessions.count

                    RailItem {
                        id: mark

                        required property int index
                        readonly property var session: Models.Sessions.sessions[index] || ({})
                        readonly property bool connected: session.connectionState === "local" || session.connectionState === "connected"

                        selected: root.selection === "terminal:" + session.id
                        Accessible.name: session.name || ""
                        contentItem: Item {
                            AgentMark {
                                anchors.centerIn: parent
                                program: mark.session.program || ""
                                size: 26
                                asleep: !mark.connected || Models.DesktopState.stagedSessionIds.indexOf(mark.session.id) < 0
                                session: mark.session
                            }
                            StatusSign {
                                anchors.right: parent.right
                                anchors.top: parent.top
                                session: mark.session
                                unseen: Models.Sessions.attention.indexOf(mark.session.id) >= 0
                                size: 9
                                backing: KodosiTheme.ground
                            }
                        }
                        onClicked: root.openTerminal(session.id)
                    }
                }
            }
        }
        Column {
            Layout.alignment: Qt.AlignHCenter
            Layout.bottomMargin: 12
            spacing: 6

            KIconButton {
                glyph: "people"
                size: 32
                active: root.selection === "people"
                Accessible.name: qsTr("People")
                onClicked: Models.DesktopState.activeView = Models.DesktopState.People
            }
            KIconButton {
                glyph: "history"
                size: 32
                Accessible.name: qsTr("Resume")
                onClicked: root.resumeRequested()
            }
            KIconButton {
                glyph: "settings"
                size: 32
                active: root.selection === "settings"
                Accessible.name: qsTr("Settings")
                onClicked: Models.DesktopState.activeView = Models.DesktopState.Settings
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        visible: !root.collapsed
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: root.titleBar ? KodosiTheme.titleBarHeight : -1
            Layout.leftMargin: 16
            Layout.rightMargin: 8
            Layout.topMargin: root.titleBar ? 3 : 14
            spacing: 2

            Wordmark {
                size: 16
                blinks: Models.Sessions.working
            }
            Item { Layout.fillWidth: true }
            KIconButton {
                glyph: "search"
                objectName: "sidebar.palette"
                Accessible.id: objectName
                Accessible.name: qsTr("Go to…")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Go to… (Ctrl+Shift+K)")
                ToolTip.delay: 600
                onClicked: root.paletteRequested()
            }
            KIconButton {
                glyph: "sidebar"
                objectName: "sidebar.toggle"
                Accessible.id: objectName
                Accessible.name: qsTr("Hide sidebar")
                onClicked: Models.DesktopState.sidebarOpen = false
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            Layout.topMargin: root.titleBar ? 10 : 14
            Layout.bottomMargin: 10
            implicitHeight: 34

            Bezel {
                anchors.fill: parent
                fill: newTerminal.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                pressed: newTerminal.pressed
            }
            AbstractButton {
                id: newTerminal
                anchors.fill: parent
                objectName: "sidebar.sessions.new"
                Accessible.id: objectName
                Accessible.name: qsTr("New terminal")
                activeFocusOnTab: true
                hoverEnabled: true
                scale: pressed ? 0.98 : 1
                contentItem: RowLayout {
                    spacing: 8
                    KIcon { Layout.leftMargin: 14; Layout.preferredWidth: 13; Layout.preferredHeight: 13; name: "plus"; color: KodosiTheme.accentInk; strokeWidth: 2.6 }
                    PlainLabel { Layout.fillWidth: true; text: qsTr("New terminal"); color: KodosiTheme.accentInk; font.weight: Font.DemiBold }
                    PlainLabel { Layout.rightMargin: 12; text: qsTr("Ctrl+Shift+N"); color: KodosiTheme.alpha(KodosiTheme.accentInk, 0.6); font.pixelSize: KodosiTheme.fontCaption }
                }
                onClicked: root.newTerminalRequested("")
            }
        }
        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: content.implicitHeight + 16
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: KScrollBar {}

            Raised {
                id: pill
                visible: false
                radius: KodosiTheme.radiusMd

                Behavior on y { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: 0.9 } }
                Behavior on height { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                Rectangle {
                    x: -1.5
                    anchors.verticalCenter: parent.verticalCenter
                    width: 3
                    height: 14
                    radius: 1.5
                    color: KodosiTheme.accent
                }
            }
            Column {
                id: content
                x: 8
                y: 8
                width: flick.width - 16
                spacing: 2

                onImplicitHeightChanged: Qt.callLater(root.syncPill)
                onWidthChanged: Qt.callLater(root.syncPill)

                SectionHeader {
                    title: qsTr("Rooms")
                    addName: qsTr("New room")
                    identifier: "sidebar.rooms"
                    selected: root.selection === "rooms"
                    onClicked: root.showRooms()
                    onAddClicked: root.accountReady ? root.newRoomRequested() : root.signIn()
                }
                AbstractButton {
                    id: signInHint
                    width: parent.width
                    height: 32
                    visible: !root.accountReady
                    hoverEnabled: true
                    objectName: "sidebar.signIn.hint"
                    Accessible.id: objectName
                    Accessible.name: hint.text
                    contentItem: RowLayout {
                        spacing: 9
                        Item {
                            Layout.leftMargin: 8
                            Layout.preferredWidth: 22
                            Layout.preferredHeight: 22
                            KIcon { anchors.centerIn: parent; width: 15; height: 15; name: "people" }
                        }
                        PlainLabel {
                            id: hint
                            Layout.fillWidth: true
                            text: Models.Account.signedIn ? qsTr("Approve this device to work together") : qsTr("Sign in to work together")
                            color: KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            elide: Text.ElideRight
                        }
                    }
                    background: Rectangle {
                        radius: KodosiTheme.radiusMd
                        color: KodosiTheme.alpha(KodosiTheme.ink, signInHint.hovered ? 0.06 : 0)
                    }
                    onClicked: root.signIn()
                }
                Repeater {
                    model: root.invitations.length

                    Item {
                        id: invitation

                        required property int index
                        readonly property var entry: root.invitations[index] || ({})

                        width: content.width
                        height: invitationColumn.implicitHeight + 24

                        Rectangle {
                            anchors.fill: parent
                            anchors.topMargin: 2
                            anchors.bottomMargin: 2
                            radius: KodosiTheme.radiusLg
                            color: KodosiTheme.alpha(KodosiTheme.accentSoft, 0.55)
                            border.width: 1
                            border.color: KodosiTheme.alpha(KodosiTheme.accent, 0.45)
                        }
                        ColumnLayout {
                            id: invitationColumn
                            x: 10
                            y: 12
                            width: parent.width - 20
                            spacing: 8

                            RowLayout {
                                spacing: 9
                                RoomSigil { key: invitation.entry.missionId || ""; size: 22 }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 1

                                    PlainLabel {
                                        Layout.fillWidth: true
                                        text: invitation.entry.missionName || ""
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    PlainLabel {
                                        Layout.fillWidth: true
                                        visible: !!invitation.entry.inviterName
                                        text: qsTr("%1 invited you").arg(invitation.entry.inviterName || "")
                                        color: KodosiTheme.inkMuted
                                        font.pixelSize: KodosiTheme.fontCaption
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                            RowLayout {
                                spacing: 6
                                KButton {
                                    compact: true
                                    variant: KButton.Primary
                                    text: qsTr("Join")
                                    objectName: "sidebar.invitation.accept." + invitation.entry.id
                                    Accessible.id: objectName
                                    onClicked: {
                                        Models.DesktopState.activeView = Models.DesktopState.Missions
                                        Models.Missions.acceptInvitation(invitation.entry.id)
                                    }
                                }
                                KButton {
                                    compact: true
                                    variant: KButton.Ghost
                                    text: qsTr("Not now")
                                    objectName: "sidebar.invitation.decline." + invitation.entry.id
                                    Accessible.id: objectName
                                    onClicked: Models.Missions.declineInvitation(invitation.entry.id)
                                }
                            }
                        }
                    }
                }
                Repeater {
                    model: root.rooms.length

                    AbstractButton {
                        id: room

                        required property int index
                        readonly property var entry: root.rooms[index] || ({})
                        readonly property bool selected: root.selection === "room:" + entry.id
                        readonly property bool unread: Models.Missions.unreadRooms.indexOf(entry.id) >= 0
                        readonly property var work: root.roomWork[entry.id] || ({ asks: false, working: false })

                        width: parent ? parent.width : 0
                        height: 34
                        hoverEnabled: true
                        activeFocusOnTab: true
                        objectName: "sidebar.room." + entry.id
                        Accessible.id: objectName
                        Accessible.name: entry.name || ""
                        Accessible.description: unread ? qsTr("New messages") : ""
                        Accessible.selected: selected

                        onSelectedChanged: root.claimPill(room, selected)
                        Component.onCompleted: root.claimPill(room, selected)
                        Component.onDestruction: root.claimPill(room, false)

                        contentItem: RowLayout {
                            spacing: 9

                            RoomSigil {
                                Layout.leftMargin: 8
                                key: room.entry.id || ""
                                size: 22
                            }
                            PlainLabel {
                                Layout.fillWidth: true
                                text: room.entry.name || ""
                                color: room.selected || room.hovered || room.unread ? KodosiTheme.ink : KodosiTheme.inkMuted
                                font.weight: room.selected || room.unread ? Font.DemiBold : Font.Medium
                                elide: Text.ElideRight
                            }
                            NewDot {
                                Layout.rightMargin: 12
                                visible: room.unread
                            }
                            StatusSign {
                                Layout.rightMargin: 8
                                visible: !room.unread && room.work.asks
                                session: ({ sign: "hand", asks: true, signLabel: qsTr("Needs your approval") })
                            }
                            BreathingDot {
                                Layout.rightMargin: 13
                                visible: !room.unread && !room.work.asks && room.work.working
                                size: 6
                                color: KodosiTheme.glowOrange
                            }
                        }
                        background: Rectangle {
                            radius: KodosiTheme.radiusMd
                            color: KodosiTheme.alpha(KodosiTheme.ink, !room.selected && (room.hovered || room.visualFocus) ? 0.06 : 0)
                        }

                        onClicked: root.openRoom(entry.id)
                    }
                }
                PlainLabel {
                    width: parent.width
                    leftPadding: 10
                    visible: Models.Missions.catalogTruncated
                    text: qsTr("More rooms exist. Use Go to… to find them.")
                    color: KodosiTheme.inkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                    wrapMode: Text.WordWrap
                }
                Item { width: 1; height: 14 }
                SectionHeader {
                    title: qsTr("Terminals")
                    addName: qsTr("New terminal")
                    identifier: "sidebar.terminals"
                    onClicked: Models.DesktopState.activeView = Models.DesktopState.Sessions
                    onAddClicked: root.newTerminalRequested("")
                }
                Repeater {
                    model: root.groups.length

                    Column {
                        id: folder

                        required property int index
                        readonly property var group: root.groups[index] || ({})
                        readonly property bool folded: root.collapsedFolders[group.key] === true

                        width: content.width
                        spacing: 2

                        AbstractButton {
                            id: folderHeader
                            width: parent.width
                            height: 24
                            hoverEnabled: true
                            objectName: "sidebar.folder." + folder.group.key
                            Accessible.id: objectName
                            Accessible.name: folderLabel.text
                            contentItem: RowLayout {
                                spacing: 6
                                KIcon {
                                    Layout.leftMargin: 10
                                    Layout.preferredWidth: 9
                                    Layout.preferredHeight: 9
                                    name: "chevron-down"
                                    strokeWidth: 2.6
                                    color: KodosiTheme.inkFaint
                                    rotation: folder.folded ? -90 : 0
                                    Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                                }
                                KIcon {
                                    visible: !!folder.group.host
                                    Layout.preferredWidth: 12
                                    Layout.preferredHeight: 12
                                    name: "laptop"
                                    color: KodosiTheme.inkFaint
                                }
                                PlainLabel {
                                    id: folderLabel
                                    Layout.fillWidth: true
                                    text: folder.group.host ? (folder.group.owner ? folder.group.owner + " · " + folder.group.host : folder.group.host) : (folder.group.name || "")
                                    color: folderHeader.hovered ? KodosiTheme.inkMuted : KodosiTheme.inkFaint
                                    font.pixelSize: KodosiTheme.fontCaption
                                    font.weight: Font.Medium
                                    elide: Text.ElideRight
                                }
                                RollingText {
                                    visible: folder.folded
                                    text: String((folder.group.sessions || []).length)
                                    color: KodosiTheme.inkFaint
                                    font.pixelSize: KodosiTheme.fontCaption
                                }
                                StatusSign {
                                    size: 11
                                    session: folder.folded
                                        ? (folder.group.sessions || []).find(entry => entry.asks) || null
                                        : null
                                }
                                KIconButton {
                                    id: branchButton
                                    size: 20
                                    glyph: "branch"
                                    visible: folder.group.repository === true
                                        && (folderHover.hovered || (branchPopover.visible && branchPopover.directory === folder.group.directory))
                                    active: branchPopover.visible && branchPopover.directory === folder.group.directory
                                    objectName: "sidebar.folder.branch." + folder.group.key
                                    Accessible.id: objectName
                                    Accessible.name: qsTr("New terminal on a new branch in %1").arg(folder.group.name)
                                    ToolTip.visible: hovered && !branchPopover.visible
                                    ToolTip.text: qsTr("New terminal on a new branch")
                                    ToolTip.delay: 600
                                    onClicked: branchPopover.openAt(branchButton, folder.group.directory)
                                }
                                KIconButton {
                                    Layout.rightMargin: 2
                                    size: 20
                                    glyph: "plus"
                                    visible: !!folder.group.directory && folderHover.hovered
                                    objectName: "sidebar.folder.new." + folder.group.key
                                    Accessible.id: objectName
                                    Accessible.name: qsTr("New terminal in %1").arg(folder.group.name)
                                    onClicked: root.newTerminalRequested(folder.group.directory)
                                }
                            }
                            HoverHandler { id: folderHover }
                            onClicked: {
                                const next = Object.assign({}, root.collapsedFolders)
                                next[folder.group.key] = !folder.folded
                                root.collapsedFolders = next
                            }
                        }
                        Repeater {
                            model: folder.folded ? 0 : (folder.group.sessions || []).length

                            AbstractButton {
                                id: row

                                required property int index
                                readonly property var session: (folder.group.sessions || [])[index] || ({})
                                readonly property string sessionId: session.id || ""
                                readonly property bool selected: root.selection === "terminal:" + sessionId
                                readonly property bool staged: Models.DesktopState.stagedSessionIds.indexOf(sessionId) >= 0
                                readonly property bool unseen: Models.Sessions.attention.indexOf(sessionId) >= 0
                                readonly property bool renaming: root.renamingId === sessionId && sessionId.length > 0
                                readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)
                                readonly property string detail: session.activity || (session.kind === "remote" && session.connectionState !== "connected" ? (session.message || qsTr("Not connected")) : "")
                                readonly property bool connected: session.connectionState === "local" || session.connectionState === "connected"
                                readonly property bool troubled: session.connectionState === "blocked"
                                    || (session.connectionState === "offline" && (session.status === "reconnecting" || !!session.message))
                                readonly property bool signed: sign.form.length > 0

                                width: parent ? parent.width : 0
                                height: 38
                                hoverEnabled: true
                                activeFocusOnTab: true
                                objectName: "sidebar.session." + sessionId
                                Accessible.id: objectName
                                Accessible.name: session.name || ""
                                Accessible.selected: selected

                                onSelectedChanged: root.claimPill(row, selected)
                                Component.onCompleted: root.claimPill(row, selected)
                                Component.onDestruction: root.claimPill(row, false)
                                onHeightChanged: Qt.callLater(root.syncPill)

                                contentItem: RowLayout {
                                    spacing: 9

                                    AgentMark {
                                        Layout.leftMargin: 8
                                        program: row.session.program || ""
                                        size: 22
                                        asleep: !row.staged || !row.connected
                                        session: row.session
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 1
                                        visible: !row.renaming

                                        PlainLabel {
                                            Layout.fillWidth: true
                                            text: row.session.name || ""
                                            color: row.selected || row.staged ? KodosiTheme.ink : KodosiTheme.inkMuted
                                            font.weight: row.selected ? Font.DemiBold : Font.Medium
                                            elide: Text.ElideRight
                                        }
                                        ActivityLine {
                                            id: caption
                                            Layout.fillWidth: true
                                            visible: shown
                                            session: row.session
                                            words: row.detail
                                            color: KodosiTheme.inkFaint
                                            pixelSize: KodosiTheme.fontCaption
                                        }
                                    }
                                    KTextField {
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: 26
                                        visible: row.renaming
                                        topPadding: 3
                                        bottomPadding: 3
                                        leftPadding: 8
                                        objectName: row.objectName + ".name"
                                        Accessible.id: objectName
                                        Accessible.name: qsTr("Terminal name")
                                        onVisibleChanged: {
                                            if (visible) {
                                                text = row.session.name || ""
                                                forceActiveFocus()
                                                selectAll()
                                            }
                                        }
                                        onAccepted: {
                                            Models.SessionActions.rename(row.sessionId, text)
                                            root.renamingId = ""
                                        }
                                        onActiveFocusChanged: {
                                            if (!activeFocus && row.renaming)
                                                root.renamingId = ""
                                        }
                                        Keys.onEscapePressed: root.renamingId = ""
                                    }
                                    AvatarStack {
                                        visible: row.viewers.length > 0 && !actions.visible && !row.renaming
                                        userIds: row.viewers
                                        size: 15
                                        limit: 2
                                        ring: row.selected ? KodosiTheme.raised : KodosiTheme.ground
                                    }
                                    StatusSign {
                                        id: sign
                                        session: row.session
                                        unseen: row.unseen
                                        visible: form.length > 0 && !actions.visible
                                    }
                                    Rectangle {
                                        Layout.rightMargin: 4
                                        implicitWidth: 6
                                        implicitHeight: 6
                                        radius: 3
                                        color: KodosiTheme.caution
                                        visible: !row.signed && row.troubled && !actions.visible
                                    }
                                    PersonAvatar {
                                        visible: !row.signed && !row.troubled && row.session.isOwner === false && !!row.session.ownerName
                                            && !actions.visible && !row.renaming
                                        name: row.session.ownerName || ""
                                        key: row.session.ownerUserId || ""
                                        size: 15
                                        Accessible.name: qsTr("Shared by %1").arg(row.session.ownerName || "")
                                    }
                                    RoomSigil {
                                        visible: !row.signed && !row.troubled && (row.session.isOwner !== false || !row.session.ownerName)
                                            && !!row.session.missionId && !actions.visible && !row.renaming
                                        key: row.session.missionId || ""
                                        size: 14
                                    }
                                    KIcon {
                                        Layout.preferredWidth: 11
                                        Layout.preferredHeight: 11
                                        visible: !row.signed && !row.troubled && (row.session.isOwner !== false || !row.session.ownerName)
                                            && !row.session.missionId && (row.session.sharedWith || []).length > 0 && !actions.visible && !row.renaming
                                        name: "people"
                                        color: KodosiTheme.inkFaint
                                    }
                                    Row {
                                        id: actions
                                        visible: (rowHover.hovered || row.visualFocus) && !row.renaming
                                        spacing: 0

                                        KIconButton {
                                            size: 24
                                            glyph: "minus"
                                            visible: row.staged
                                            objectName: row.objectName + ".minimize"
                                            Accessible.id: objectName
                                            Accessible.name: qsTr("Minimize")
                                            onClicked: Models.SessionActions.minimize(row.sessionId)
                                        }
                                        KIconButton {
                                            size: 24
                                            glyph: "close"
                                            destructive: true
                                            enabled: row.session.canControl === true
                                            objectName: row.objectName + ".close"
                                            Accessible.id: objectName
                                            Accessible.name: qsTr("Close terminal")
                                            onClicked: closeDialog.ask(row.sessionId, row.session.name || "")
                                        }
                                    }
                                    Item {
                                        Layout.preferredWidth: 2
                                    }
                                }
                                background: Rectangle {
                                    radius: KodosiTheme.radiusMd
                                    color: KodosiTheme.alpha(KodosiTheme.ink, !row.selected && (rowHover.hovered || row.visualFocus) ? 0.06 : 0)
                                }

                                onClicked: root.openTerminal(sessionId)
                                onDoubleClicked: {
                                    if (row.session.isOwner === true)
                                        root.renamingId = sessionId
                                }

                                HoverHandler { id: rowHover }
                                TapHandler {
                                    acceptedButtons: Qt.RightButton
                                    onTapped: {
                                        menu.sessionId = row.sessionId
                                        menu.popup()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Column {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.topMargin: 8
            Layout.bottomMargin: 10
            spacing: 2

            NavRow {
                title: qsTr("People")
                iconName: "people"
                badge: Models.People.incoming.length
                selected: root.selection === "people"
                objectName: "sidebar.people"
                onClicked: Models.DesktopState.activeView = Models.DesktopState.People
            }
            Item { width: 1; height: 4 }
            RowLayout {
                width: parent.width
                spacing: 6

                Item {
                    id: signInSplit

                    readonly property bool approving: Models.Account.signedIn

                    Layout.fillWidth: true
                    implicitHeight: 34
                    visible: !root.accountReady

                    Bezel {
                        anchors.fill: parent
                        quiet: true
                        pressed: signInButton.pressed || settingsButton.pressed
                        fill: signInSplit.approving
                            ? KodosiTheme.mix(KodosiTheme.accentSoft, KodosiTheme.accent, splitHover.hovered ? 0.14 : 0)
                            : KodosiTheme.mix(KodosiTheme.lifted, KodosiTheme.ink, (KodosiTheme.isDark ? 0.07 : 0) + (splitHover.hovered ? 0.03 : 0))
                    }
                    HoverHandler { id: splitHover }
                    RowLayout {
                        anchors.fill: parent
                        spacing: 0

                        AbstractButton {
                            id: signInButton

                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            activeFocusOnTab: true
                            hoverEnabled: true
                            scale: pressed ? 0.98 : 1
                            objectName: "sidebar.signIn"
                            Accessible.id: objectName
                            Accessible.name: signInLabel.text
                            contentItem: RowLayout {
                                spacing: 9

                                SlotMark {
                                    Layout.leftMargin: 8
                                    visible: !signInSplit.approving
                                    glyph: "person"
                                    tint: KodosiTheme.inkMuted
                                    size: 20
                                }
                                KIcon {
                                    Layout.leftMargin: 10
                                    Layout.preferredWidth: 16
                                    Layout.preferredHeight: 16
                                    visible: signInSplit.approving
                                    name: "lock"
                                    color: KodosiTheme.accentStrong
                                    strokeWidth: 2
                                }
                                ShimmerText {
                                    id: signInLabel

                                    Layout.fillWidth: true
                                    text: signInSplit.approving ? qsTr("Approve this device") : Models.Account.signingIn ? qsTr("Signing in…") : qsTr("Sign in")
                                    active: Models.Account.signingIn
                                    color: signInSplit.approving ? KodosiTheme.accentStrong : KodosiTheme.ink
                                    font.pixelSize: KodosiTheme.fontBody
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                }
                            }
                            onClicked: root.signIn()

                            KFocusIndicator { active: signInButton.visualFocus }
                        }
                        Rectangle {
                            Layout.preferredWidth: 1
                            Layout.preferredHeight: 18
                            color: KodosiTheme.alpha(KodosiTheme.ink, 0.12)
                        }
                        AbstractButton {
                            id: settingsButton

                            Layout.preferredWidth: 38
                            Layout.fillHeight: true
                            activeFocusOnTab: true
                            hoverEnabled: true
                            scale: pressed ? 0.94 : 1
                            objectName: "sidebar.settings.icon"
                            Accessible.id: objectName
                            Accessible.name: qsTr("Settings")
                            Accessible.selected: root.selection === "settings"
                            contentItem: Item {
                                KIcon {
                                    anchors.centerIn: parent
                                    width: 15
                                    height: 15
                                    name: "settings"
                                    color: root.selection === "settings" ? KodosiTheme.accentStrong
                                        : settingsButton.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                    strokeWidth: 1.9
                                }
                            }
                            onClicked: Models.DesktopState.activeView = Models.DesktopState.Settings

                            KFocusIndicator { active: settingsButton.visualFocus }
                        }
                    }
                }
                AbstractButton {
                    id: account
                    Layout.fillWidth: true
                    implicitHeight: 38
                    visible: root.accountReady
                    hoverEnabled: true
                    activeFocusOnTab: true
                    objectName: "sidebar.settings"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Settings")
                    contentItem: RowLayout {
                        spacing: 9
                        PersonAvatar { Layout.leftMargin: 8; name: Identity.selfName; size: 24; isSelf: true }
                        PlainLabel { Layout.fillWidth: true; text: Identity.selfName; font.weight: Font.DemiBold; elide: Text.ElideRight }
                        KIcon { Layout.rightMargin: 10; Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "settings"; color: KodosiTheme.inkFaint }
                    }
                    background: Item {
                        Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; visible: root.selection === "settings" }
                        Rectangle {
                            anchors.fill: parent
                            radius: KodosiTheme.radiusLg
                            color: KodosiTheme.alpha(KodosiTheme.ink, root.selection !== "settings" && account.hovered ? 0.06 : 0)
                        }
                    }
                    onClicked: Models.DesktopState.activeView = Models.DesktopState.Settings
                }
            }
        }
    }

    KMenu {
        id: menu

        property string sessionId: ""
        readonly property var session: Models.Sessions.presentationForSession(sessionId)

        KMenuItem { text: qsTr("Open"); iconName: "terminal"; onTriggered: root.openTerminal(menu.sessionId) }
        KMenuItem { text: qsTr("Rename"); iconName: "pencil"; enabled: menu.session.isOwner === true; onTriggered: root.renamingId = menu.sessionId }
        KMenuItem { text: qsTr("Share…"); iconName: "share"; enabled: menu.session.kind === "local" && menu.session.isOwner === true; onTriggered: root.shareRequested(menu.sessionId) }
        KMenuItem { text: qsTr("Details"); iconName: "info"; onTriggered: root.detailsRequested(menu.sessionId) }
        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.alpha(KodosiTheme.hairline, 0.7) } }
        KMenuItem { text: qsTr("Minimize"); iconName: "minus"; enabled: Models.DesktopState.stagedSessionIds.indexOf(menu.sessionId) >= 0; onTriggered: Models.SessionActions.minimize(menu.sessionId) }
        KMenuItem { text: qsTr("Close…"); iconName: "close"; destructive: true; enabled: menu.session.canControl === true; onTriggered: closeDialog.ask(menu.sessionId, menu.session.name || "") }
    }
    KPopover {
        id: branchPopover

        property string directory: ""

        function openAt(item, folder) {
            const point = item.mapToItem(root, item.width + 8, -14)
            x = point.x
            y = point.y
            directory = folder
            branchName.text = Models.SessionActions.suggestedBranch()
            open()
        }
        function start() {
            if (Models.SessionActions.createOnBranch(directory, branchName.text))
                close()
        }

        objectName: "sidebar.branch"
        width: 280
        padding: 16
        focus: true
        onOpened: {
            branchName.selectAll()
            branchName.forceActiveFocus(Qt.PopupFocusReason)
        }
        contentItem: ColumnLayout {
            spacing: 10

            RowLayout {
                spacing: 8

                KIcon { Layout.preferredWidth: 15; Layout.preferredHeight: 15; name: "branch"; color: KodosiTheme.inkMuted; strokeWidth: 2 }
                PlainLabel { text: qsTr("New branch"); color: KodosiTheme.ink; font.pixelSize: KodosiTheme.fontHeadline; font.weight: Font.DemiBold }
            }
            KTextField {
                id: branchName

                Layout.fillWidth: true
                objectName: "sidebar.branch.name"
                Accessible.id: objectName
                Accessible.name: qsTr("Branch name")
                placeholderText: qsTr("Branch name")
                onAccepted: branchPopover.start()
            }
            PlainLabel {
                Layout.fillWidth: true
                text: qsTr("The branch gets its own folder, and a terminal starts there.")
                color: KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
                wrapMode: Text.WordWrap
            }
            KButton {
                Layout.fillWidth: true
                objectName: "sidebar.branch.start"
                Accessible.id: objectName
                variant: KButton.Primary
                enabled: branchName.text.trim().length > 0
                text: qsTr("Start")
                onClicked: branchPopover.start()
            }
        }
    }
    CloseTerminalDialog {
        id: closeDialog
    }
}
