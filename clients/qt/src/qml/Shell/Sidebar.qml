pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property bool collapsed: !Models.DesktopState.sidebarOpen
    readonly property bool dosi: Models.DesktopState.dosiOpen && !collapsed
    readonly property var pins: Models.DesktopState.pinnedSessionIds
    readonly property var groups: {
        const all = Models.Sessions.folderGroups
        const pinned = []
        for (const id of pins) {
            for (const group of all) {
                const session = (group.sessions || []).find(entry => entry.id === id)
                if (session) {
                    pinned.push(session)
                    break
                }
            }
        }
        const rest = all.map(group => Object.assign({}, group, { sessions: (group.sessions || []).filter(entry => pins.indexOf(entry.id) < 0) }))
            .filter(group => group.sessions.length > 0)
        let previous = null
        for (const group of rest) {
            group.computerStart = group.computer !== previous
            previous = group.computer
        }
        return pinned.length > 0 ? [{ key: "pinned", name: qsTr("Pinned"), pinned: true, folder: true, sessions: pinned }].concat(rest) : rest
    }
    readonly property bool titled: groups.some(group => !group.pinned && group.computer !== "local")
    readonly property var rooms: Models.Missions.missions
    readonly property var invitations: Models.Missions.invitations
    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property var approvalSign: ({ sign: "hand", asks: true, signLabel: qsTr("Needs your approval") })
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

    signal newTerminalRequested
    signal detailsRequested(string sessionId)
    signal shareRequested(string sessionId)
    signal resumeRequested(string provider)
    signal paletteRequested
    signal newRoomRequested
    signal dosiSettingsRequested
    signal accountSettingsRequested
    signal aboutRequested

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
    implicitWidth: Models.DesktopState.sidebarWidth

    Component.onCompleted: Fold.stage = root
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
                color: titleButton.hovered ? KodosiTheme.ink : KodosiTheme.inkFaint
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
            ToolTip.delay: Tip.delay
            onClicked: header.addClicked()
        }
    }

    component RailItem: AbstractButton {
        id: item

        property bool selected: false

        anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
        width: 40
        height: 40
        hoverEnabled: true
        scale: pressed ? 0.95 : 1
        Accessible.selected: selected
        ToolTip.visible: hovered
        ToolTip.text: Accessible.name
        ToolTip.delay: Tip.delay

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
                scale: pressed ? 0.95 : 1
                objectName: "rail.newTerminal"
                Accessible.id: objectName
                Accessible.name: qsTr("New terminal")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("New terminal (Ctrl+Shift+N)")
                ToolTip.delay: Tip.delay
                background: Bezel {
                    radius: KodosiTheme.radiusMd
                    fill: railNew.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                    pressed: railNew.pressed
                    opacity: railNewSpot.hidden ? 0 : 1

                    FoldSpot {
                        id: railNewSpot
                        anchors.fill: parent
                        key: "new"
                        place: "rail"
                        fill: KodosiTheme.accent
                        radius: KodosiTheme.radiusMd
                    }
                }
                contentItem: Item {
                    KIcon { anchors.centerIn: parent; width: 14; height: 14; name: "plus"; color: KodosiTheme.accentInk; strokeWidth: 2.6; opacity: railNewSpot.hidden ? 0 : 1 }
                }
                onClicked: root.newTerminalRequested()
            }
        }
        Flickable {
            id: railFlick
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
                                opacity: railRoomSpot.hidden ? 0 : 1

                                FoldSpot {
                                    id: railRoomSpot
                                    anchors.fill: parent
                                    key: "room:" + (railRoom.entry.id || "")
                                    place: "rail"
                                    sigil: railRoom.entry.id || ""
                                    view: railFlick
                                }
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
                    visible: root.rooms.length > 0 && Models.Sessions.listedSessions.length > 0
                }
                Repeater {
                    model: Models.Sessions.listedSessions.length

                    RailItem {
                        id: mark

                        required property int index
                        readonly property var session: Models.Sessions.listedSessions[index] || ({})
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
                                opacity: railMarkSpot.hidden ? 0 : 1

                                FoldSpot {
                                    id: railMarkSpot
                                    anchors.fill: parent
                                    key: "terminal:" + (mark.session.id || "")
                                    place: "rail"
                                    session: mark.session
                                    asleep: !mark.connected || Models.DesktopState.stagedSessionIds.indexOf(mark.session.id) < 0
                                    view: railFlick
                                }
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
        DosiIsland {
            Layout.alignment: Qt.AlignHCenter
            visible: Models.DesktopState.dosiOn
        }
        Column {
            Layout.alignment: Qt.AlignHCenter
            Layout.bottomMargin: 12
            spacing: 6
            opacity: railFoot.progress
            transform: Arrive { id: railFoot; delay: 250; rise: 0 }
            onVisibleChanged: {
                if (visible)
                    railFoot.play()
            }

            RailItem {
                objectName: root.accountReady ? "sidebar.account" : "sidebar.signIn"
                Accessible.id: objectName
                Accessible.name: root.accountReady ? Identity.selfName : signInLabel.text
                contentItem: Item {
                    PersonAvatar {
                        anchors.centerIn: parent
                        visible: root.accountReady
                        name: Identity.selfName
                        size: 26
                        isSelf: true
                    }
                    SlotMark {
                        anchors.centerIn: parent
                        visible: !Models.Account.signedIn
                        glyph: "person"
                        tint: KodosiTheme.inkMuted
                        size: 26
                    }
                    KIcon {
                        anchors.centerIn: parent
                        width: 16
                        height: 16
                        visible: Models.Account.signedIn && !root.accountReady
                        name: "lock"
                        color: KodosiTheme.inkMuted
                        strokeWidth: 2
                    }
                    StatusSign {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        session: Models.Account.signedIn && (!root.accountReady || Models.Devices.requests.length > 0)
                            ? root.approvalSign : null
                        size: 9
                        backing: KodosiTheme.ground
                    }
                }
                onClicked: root.accountReady ? root.accountSettingsRequested() : root.signIn()
            }
            SidebarMenu {
                anchors.horizontalCenter: parent.horizontalCenter
                opensRight: true
                selected: root.selection === "people" || root.selection === "settings"
                onAboutRequested: root.aboutRequested()
                onResumeRequested: provider => root.resumeRequested(provider)
                onStartRequested: root.newTerminalRequested()
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
            Layout.leftMargin: root.dosi ? 10 : 16
            Layout.rightMargin: 8
            Layout.topMargin: root.titleBar ? 3 : 14
            spacing: 2

            AbstractButton {
                id: back

                Layout.preferredWidth: 26
                Layout.preferredHeight: 26
                Layout.rightMargin: 6
                visible: root.dosi
                hoverEnabled: true
                activeFocusOnTab: true
                scale: pressed ? 0.95 : 1
                objectName: "dosi.close"
                Accessible.id: objectName
                Accessible.name: qsTr("Back to terminals and rooms")
                ToolTip.visible: hovered
                ToolTip.text: Accessible.name
                ToolTip.delay: Tip.delay
                onClicked: Models.DesktopState.dosiOpen = false

                Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                background: Bezel {
                    radius: KodosiTheme.radiusSm
                    quiet: true
                    pressed: back.pressed
                    fill: KodosiTheme.buttonFace(back.hovered)
                }
                contentItem: Item {
                    KIcon {
                        anchors.centerIn: parent
                        anchors.horizontalCenterOffset: -0.5
                        width: 13
                        height: 13
                        name: "chevron-left"
                        color: back.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                        strokeWidth: 2.4
                    }
                }

                KFocusIndicator { active: back.visualFocus }
            }
            Wordmark {
                size: 16
            }
            Item { Layout.fillWidth: true }
            DosiNewChat {
                visible: root.dosi && !Models.Dosi.empty
            }
            KIconButton {
                glyph: "search"
                objectName: "sidebar.palette"
                Accessible.id: objectName
                Accessible.name: qsTr("Go to…")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Go to… (Ctrl+Shift+K)")
                ToolTip.delay: Tip.delay
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
            visible: !root.dosi

            Bezel {
                anchors.fill: parent
                fill: newTerminal.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                pressed: newTerminal.pressed
                opacity: newSpot.hidden ? 0 : 1

                FoldSpot {
                    id: newSpot
                    anchors.fill: parent
                    key: "new"
                    place: "row"
                    fill: KodosiTheme.accent
                    radius: KodosiTheme.radiusMd
                }
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
                    opacity: newSpot.hidden ? 0 : 1
                    KIcon { Layout.leftMargin: 14; Layout.preferredWidth: 13; Layout.preferredHeight: 13; name: "plus"; color: KodosiTheme.accentInk; strokeWidth: 2.6 }
                    PlainLabel { Layout.fillWidth: true; text: qsTr("New terminal"); color: KodosiTheme.accentInk; font.weight: Font.DemiBold }
                    PlainLabel { Layout.rightMargin: 12; text: qsTr("Ctrl+Shift+N"); color: KodosiTheme.alpha(KodosiTheme.accentInk, 0.6); font.pixelSize: KodosiTheme.fontCaption }
                }
                onClicked: root.newTerminalRequested()
            }
        }
        DosiPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.dosi
            onNewTerminalRequested: root.newTerminalRequested()
            onNewRoomRequested: root.accountReady ? root.newRoomRequested() : root.signIn()
            onSettingsRequested: root.dosiSettingsRequested()
        }
        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: content.implicitHeight + 16
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: KScrollBar {}
            visible: !root.dosi
            opacity: listArrive.progress
            transform: Arrive { id: listArrive; delay: 220; rise: 0 }
            onVisibleChanged: {
                if (visible)
                    listArrive.play()
            }

            Raised {
                id: pill
                visible: false
                radius: KodosiTheme.radiusMd

                Behavior on y { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
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
                                opacity: roomSpot.hidden ? 0 : 1

                                FoldSpot {
                                    id: roomSpot
                                    anchors.fill: parent
                                    key: "room:" + (room.entry.id || "")
                                    place: "row"
                                    sigil: room.entry.id || ""
                                    view: flick
                                }
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

                        TerminalDropArea {
                            anchors.fill: parent
                            objectName: room.objectName + ".drop"
                            onTerminalDropped: id => roomShare.ask(id, room.entry.id)
                        }
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
                    onAddClicked: root.newTerminalRequested()
                }
                Repeater {
                    model: root.groups.length

                    Column {
                        id: folder

                        required property int index
                        readonly property var group: root.groups[index] || ({})
                        readonly property bool folded: group.folder === true && root.collapsedFolders[group.key] === true

                        width: content.width
                        spacing: 2
                        onYChanged: Qt.callLater(root.syncPill)

                        Item {
                            width: parent.width
                            height: 30
                            visible: root.titled && folder.group.computerStart === true && !folder.group.pinned
                            objectName: "sidebar.computer." + (folder.group.computer || "")
                            Accessible.id: objectName
                            Accessible.role: Accessible.Heading
                            Accessible.name: folder.group.computerOffline === true
                                ? qsTr("%1 is offline").arg(folder.group.computerTitle || "") : folder.group.computerTitle || ""

                            RowLayout {
                                anchors.left: parent.left
                                anchors.leftMargin: 10
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: 3
                                spacing: 5

                                PersonAvatar {
                                    visible: !!folder.group.owner
                                    name: folder.group.owner || ""
                                    key: folder.group.ownerUserId || ""
                                    size: 13
                                }
                                KIcon {
                                    visible: !folder.group.owner
                                    Layout.preferredWidth: 12
                                    Layout.preferredHeight: 12
                                    name: "laptop"
                                    color: KodosiTheme.inkFaint
                                }
                                PlainLabel {
                                    Layout.fillWidth: true
                                    text: folder.group.computerTitle || ""
                                    color: KodosiTheme.inkFaint
                                    font.pixelSize: KodosiTheme.fontCaption
                                    font.weight: Font.Medium
                                    elide: Text.ElideRight
                                }
                                KIcon {
                                    visible: folder.group.computerOffline === true
                                    Layout.preferredWidth: 10
                                    Layout.preferredHeight: 10
                                    name: "offline"
                                    color: KodosiTheme.inkFaint
                                }
                            }
                        }
                        AbstractButton {
                            id: folderHeader
                            width: parent.width
                            height: 24
                            visible: folder.group.folder === true
                            hoverEnabled: true
                            objectName: "sidebar.folder." + folder.group.key
                            Accessible.id: objectName
                            Accessible.name: folder.group.host ? qsTr("%1 on %2").arg(folderLabel.text).arg(folder.group.host) : folderLabel.text
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
                                    Layout.preferredWidth: 12
                                    Layout.preferredHeight: 12
                                    name: folder.group.pinned ? "pin" : "folder"
                                    color: KodosiTheme.inkFaint
                                }
                                PlainLabel {
                                    id: folderLabel
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: Math.min(implicitWidth, 120)
                                    text: folder.group.name || ""
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
                                    open: branchPopover.visible && branchPopover.directory === folder.group.directory
                                    objectName: "sidebar.folder.branch." + folder.group.key
                                    Accessible.id: objectName
                                    Accessible.name: qsTr("New terminal on a new branch in %1").arg(folder.group.name)
                                    ToolTip.visible: hovered && !branchPopover.visible
                                    ToolTip.text: qsTr("New terminal on a new branch")
                                    ToolTip.delay: Tip.delay
                                    onClicked: branchPopover.openAt(branchButton, folder.group.directory)
                                }
                                KIconButton {
                                    Layout.rightMargin: 2
                                    size: 20
                                    glyph: "plus"
                                    visible: folder.group.canStart === true && folderHover.hovered
                                    objectName: "sidebar.folder.new." + folder.group.key
                                    Accessible.id: objectName
                                    Accessible.name: qsTr("New terminal in %1").arg(folder.group.name)
                                    onClicked: Models.SessionActions.create("", folder.group.directory, "", "", "", ({}), folder.group.computerId || "")
                                }
                                KIconButton {
                                    Layout.rightMargin: 2
                                    size: 20
                                    glyph: "close"
                                    destructive: true
                                    visible: folder.group.offline === true && folderHover.hovered
                                    objectName: "sidebar.folder.close." + folder.group.key
                                    Accessible.id: objectName
                                    Accessible.name: qsTr("Close terminals")
                                    onClicked: closeDialog.askOffline((folder.group.sessions || []).map(session => session.id), folder.group.host || "")
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
                                readonly property bool sharedBy: folder.group.pinned === true && session.isOwner === false && !!session.ownerName
                                readonly property string sessionId: session.id || ""
                                readonly property bool selected: root.selection === "terminal:" + sessionId
                                readonly property bool staged: Models.DesktopState.stagedSessionIds.indexOf(sessionId) >= 0
                                readonly property bool unseen: Models.Sessions.attention.indexOf(sessionId) >= 0
                                readonly property bool renaming: root.renamingId === sessionId && sessionId.length > 0
                                readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)
                                readonly property string detail: session.kind === "remote" && session.status === "offline"
                                    ? qsTr("%1 is offline").arg(session.hostLabel || qsTr("Remote computer"))
                                    : session.activity || (session.kind === "remote" && session.connectionState !== "connected" ? (session.message || qsTr("Not connected")) : "")
                                readonly property bool connected: session.connectionState === "local" || session.connectionState === "connected"
                                readonly property bool troubled: session.connectionState === "blocked" || session.status === "offline"
                                    || (session.connectionState === "offline" && !!session.message)
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
                                onYChanged: Qt.callLater(root.syncPill)
                                onHeightChanged: Qt.callLater(root.syncPill)

                                contentItem: RowLayout {
                                    spacing: 9

                                    AgentMark {
                                        Layout.leftMargin: 8
                                        program: row.session.program || ""
                                        size: 22
                                        asleep: !row.staged || !row.connected
                                        session: row.session
                                        opacity: rowSpot.hidden ? 0 : 1

                                        ShareBadge {
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            anchors.rightMargin: -4
                                            anchors.bottomMargin: -4
                                            visible: !!row.session.missionId
                                                || (row.session.isOwner !== false && (row.session.sharedWith || []).length > 0)
                                            room: row.session.missionId || ""
                                            ring: row.selected ? KodosiTheme.raised : KodosiTheme.ground
                                        }
                                        FoldSpot {
                                            id: rowSpot
                                            anchors.fill: parent
                                            key: "terminal:" + row.sessionId
                                            place: "row"
                                            session: row.session
                                            asleep: !row.staged || !row.connected
                                            view: flick
                                        }
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
                                        Item {
                                            id: caption

                                            readonly property bool shown: place.shown || activity.shown

                                            Layout.fillWidth: true
                                            implicitHeight: Math.max(place.implicitHeight, activity.implicitHeight)
                                            visible: shown

                                            SessionPlace {
                                                id: place
                                                width: Math.min(implicitWidth, activity.shown ? caption.width * 0.6 : caption.width)
                                                height: caption.height
                                                session: row.session
                                                color: KodosiTheme.inkFaint
                                            }
                                            PlainLabel {
                                                id: dot
                                                x: place.width + 4
                                                anchors.verticalCenter: parent.verticalCenter
                                                visible: place.shown && activity.shown
                                                text: "\u00b7"
                                                color: KodosiTheme.inkFaint
                                                font.pixelSize: KodosiTheme.fontCaption
                                            }
                                            ActivityLine {
                                                id: activity
                                                x: place.shown ? dot.x + dot.implicitWidth + 4 : 0
                                                width: Math.max(0, caption.width - x)
                                                height: caption.height
                                                session: row.session
                                                words: row.detail
                                                color: KodosiTheme.inkFaint
                                                pixelSize: KodosiTheme.fontCaption
                                            }
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
                                        visible: !row.signed && !row.troubled && row.sharedBy
                                            && !actions.visible && !row.renaming
                                        name: row.session.ownerName || ""
                                        key: row.session.ownerUserId || ""
                                        size: 15
                                        Accessible.name: qsTr("Shared by %1").arg(row.session.ownerName || "")
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
                                            enabled: row.session.canClose === true
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
                                        terminalMenu.sessionId = row.sessionId
                                        terminalMenu.popup()
                                    }
                                }
                                TerminalDragSource {
                                    anchors.fill: parent
                                    enabled: !row.renaming
                                    sessionId: row.sessionId
                                }
                            }
                        }
                    }
                }
            }
        }
        DosiRow {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.topMargin: 6
            visible: !root.dosi && Models.DesktopState.dosiOn
        }
        Column {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.topMargin: 8
            Layout.bottomMargin: 10
            spacing: 2
            visible: !root.dosi
            opacity: footArrive.progress
            transform: Arrive { id: footArrive; delay: 280; rise: 0 }
            onVisibleChanged: {
                if (visible)
                    footArrive.play()
            }

            RowLayout {
                width: parent.width
                spacing: 6

                AbstractButton {
                    id: signInButton

                    readonly property bool approving: Models.Account.signedIn

                    Layout.fillWidth: true
                    implicitHeight: 34
                    visible: !root.accountReady
                    activeFocusOnTab: true
                    hoverEnabled: true
                    scale: pressed ? 0.97 : 1
                    objectName: "sidebar.signIn"
                    Accessible.id: objectName
                    Accessible.name: signInLabel.text

                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                    background: Bezel {
                        quiet: true
                        pressed: signInButton.pressed
                        fill: KodosiTheme.buttonFace(signInButton.hovered)
                    }
                    contentItem: RowLayout {
                        spacing: 9

                        SlotMark {
                            Layout.leftMargin: 8
                            visible: !signInButton.approving
                            glyph: "person"
                            tint: KodosiTheme.inkMuted
                            size: 20
                        }
                        ShimmerText {
                            id: signInLabel

                            Layout.fillWidth: true
                            Layout.leftMargin: signInButton.approving ? 12 : 0
                            text: signInButton.approving ? qsTr("Approve this device")
                                : Models.Account.signingIn || Models.Account.restoring.length > 0 ? qsTr("Signing in…") : qsTr("Sign in")
                            active: Models.Account.signingIn || Models.Account.restoring.length > 0
                            color: KodosiTheme.ink
                            font.pixelSize: KodosiTheme.fontBody
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        StatusSign {
                            Layout.rightMargin: 8
                            session: signInButton.approving ? root.approvalSign : null
                        }
                    }
                    onClicked: root.signIn()

                    KFocusIndicator { active: signInButton.visualFocus }
                }
                AbstractButton {
                    id: account

                    Layout.fillWidth: true
                    implicitHeight: 34
                    visible: root.accountReady
                    hoverEnabled: true
                    activeFocusOnTab: true
                    scale: pressed ? 0.98 : 1
                    objectName: "sidebar.account"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Account")
                    ToolTip.visible: hovered
                    ToolTip.text: Accessible.name
                    ToolTip.delay: Tip.delay

                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                    contentItem: RowLayout {
                        spacing: 9
                        PersonAvatar { Layout.leftMargin: 5; name: Identity.selfName; size: 24; isSelf: true }
                        PlainLabel { Layout.fillWidth: true; text: Identity.selfName; font.weight: Font.DemiBold; elide: Text.ElideRight }
                        StatusSign {
                            Layout.rightMargin: 6
                            session: Models.Devices.requests.length > 0 ? root.approvalSign : null
                        }
                    }
                    background: Rectangle {
                        radius: KodosiTheme.radiusMd
                        color: KodosiTheme.alpha(KodosiTheme.ink, account.hovered ? 0.06 : 0)
                    }
                    onClicked: root.accountSettingsRequested()

                    KFocusIndicator { active: account.visualFocus }
                }
                SidebarMenu {
                    selected: root.selection === "people" || root.selection === "settings"
                    onAboutRequested: root.aboutRequested()
                    onResumeRequested: provider => root.resumeRequested(provider)
                    onStartRequested: root.newTerminalRequested()
                }
            }
        }
    }

    HoverHandler {
        onHoveredChanged: Fold.pointing = hovered
        onPointChanged: Fold.pointer = point.position
    }
    MouseArea {
        id: resizer

        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.rightMargin: -4
        width: 8
        visible: !root.collapsed
        cursorShape: Qt.SizeHorCursor
        preventStealing: true
        Accessible.ignored: true
        onPositionChanged: mouse => {
            if (pressed)
                Models.DesktopState.resizeSidebar(mapToItem(root, mouse.x, 0).x)
        }
    }
    DosiFold {
        anchors.fill: parent
        z: 10
    }
    KMenu {
        id: terminalMenu

        property string sessionId: ""
        readonly property var session: Models.Sessions.presentationForSession(sessionId)

        KMenuItem { text: qsTr("Open"); iconName: "terminal"; onTriggered: root.openTerminal(terminalMenu.sessionId) }
        KMenuItem {
            text: root.pins.indexOf(terminalMenu.sessionId) >= 0 ? qsTr("Unpin") : qsTr("Pin")
            iconName: "pin"
            objectName: "sidebar.terminal.pin"
            onTriggered: Models.DesktopState.togglePin(terminalMenu.sessionId)
        }
        KMenuItem { text: qsTr("Rename"); iconName: "pencil"; enabled: terminalMenu.session.isOwner === true; onTriggered: root.renamingId = terminalMenu.sessionId }
        KMenuItem { text: qsTr("Share…"); iconName: "share"; enabled: terminalMenu.session.kind === "local" && terminalMenu.session.isOwner === true; onTriggered: root.shareRequested(terminalMenu.sessionId) }
        KMenuItem { text: qsTr("Details"); iconName: "info"; onTriggered: root.detailsRequested(terminalMenu.sessionId) }
        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
        KMenuItem { text: qsTr("Minimize"); iconName: "minus"; enabled: Models.DesktopState.stagedSessionIds.indexOf(terminalMenu.sessionId) >= 0; onTriggered: Models.SessionActions.minimize(terminalMenu.sessionId) }
        KMenuItem { text: qsTr("Close…"); iconName: "close"; destructive: true; enabled: terminalMenu.session.canClose === true; onTriggered: closeDialog.ask(terminalMenu.sessionId, terminalMenu.session.name || "") }
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
        transformOrigin: Popup.TopLeft
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
    RoomShareDialog {
        id: roomShare
        onShared: id => root.openRoom(id)
    }
}
