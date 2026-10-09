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
    readonly property string selection: {
        switch (Models.DesktopState.activeView) {
        case Models.DesktopState.Sessions: return Models.DesktopState.selectedSessionId.length > 0 ? "terminal:" + Models.DesktopState.selectedSessionId : ""
        case Models.DesktopState.Missions: return Models.Missions.selectedMissionId.length > 0 ? "room:" + Models.Missions.selectedMissionId : "rooms"
        case Models.DesktopState.People: return "people"
        default: return "settings"
        }
    }
    readonly property var waiting: Models.Sessions.sessions.filter(session => Models.Sessions.attention.indexOf(session.id) >= 0)
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

    Flickable {
        id: rail
        anchors.fill: parent
        visible: root.collapsed
        contentHeight: railColumn.implicitHeight + 20
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: railColumn
            y: 10
            width: parent.width
            spacing: 8

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
                objectName: "rail.newTerminal"
                Accessible.id: objectName
                Accessible.name: qsTr("New terminal")
                background: Raised {
                    radius: 17
                    fill: railNew.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                }
                contentItem: Item {
                    KIcon { anchors.centerIn: parent; width: 14; height: 14; name: "plus"; color: KodosiTheme.accentInk; strokeWidth: 2.6 }
                }
                onClicked: root.newTerminalRequested("")
            }
            Item { width: 1; height: 2 }
            Repeater {
                model: root.rooms.length

                AbstractButton {
                    id: railRoom

                    required property int index
                    readonly property var entry: root.rooms[index] || ({})

                    anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
                    width: 40
                    height: 40
                    Accessible.name: entry.name || ""
                    ToolTip.visible: hovered
                    ToolTip.text: entry.name || ""
                    hoverEnabled: true
                    background: Raised {
                        radius: KodosiTheme.radiusMd
                        visible: root.selection === "room:" + railRoom.entry.id
                    }
                    contentItem: Item {
                        RoomSigil { anchors.centerIn: parent; key: railRoom.entry.id || ""; size: 26 }
                    }
                    onClicked: root.openRoom(entry.id)
                }
            }
            Repeater {
                model: Models.Sessions.sessions.length

                AbstractButton {
                    id: mark

                    required property int index
                    readonly property var session: Models.Sessions.sessions[index] || ({})

                    anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
                    width: 40
                    height: 40
                    Accessible.name: session.name || ""
                    ToolTip.visible: hovered
                    ToolTip.text: session.name || ""
                    hoverEnabled: true
                    background: Raised {
                        radius: KodosiTheme.radiusMd
                        visible: root.selection === "terminal:" + mark.session.id
                    }
                    contentItem: Item {
                        AgentMark {
                            anchors.centerIn: parent
                            program: mark.session.program || ""
                            size: 26
                            asleep: Models.DesktopState.stagedSessionIds.indexOf(mark.session.id) < 0
                            working: mark.session.working === true
                        }
                        BreathingDot {
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 3
                            visible: Models.Sessions.attention.indexOf(mark.session.id) >= 0
                        }
                    }
                    onClicked: root.openTerminal(session.id)
                }
            }
            Item { width: 1; height: 6 }
            KIconButton {
                anchors.horizontalCenter: parent.horizontalCenter
                glyph: "people"
                active: root.selection === "people"
                Accessible.name: qsTr("People")
                onClicked: Models.DesktopState.activeView = Models.DesktopState.People
            }
            KIconButton {
                anchors.horizontalCenter: parent.horizontalCenter
                glyph: "history"
                Accessible.name: qsTr("Resume")
                onClicked: root.resumeRequested()
            }
            KIconButton {
                anchors.horizontalCenter: parent.horizontalCenter
                glyph: "settings"
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
            Layout.leftMargin: 16
            Layout.rightMargin: 8
            Layout.topMargin: 14
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
                ToolTip.text: qsTr("Go to… (Ctrl+K)")
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
            Layout.topMargin: 14
            Layout.bottomMargin: 10
            implicitHeight: 34

            Raised {
                anchors.fill: parent
                radius: 17
                fill: newHover.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
            }
            HoverHandler { id: newHover }
            RowLayout {
                anchors.fill: parent
                spacing: 0

                AbstractButton {
                    id: newTerminal
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    objectName: "sidebar.sessions.new"
                    Accessible.id: objectName
                    Accessible.name: qsTr("New terminal")
                    activeFocusOnTab: true
                    hoverEnabled: true
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("New terminal (Ctrl+Shift+N)")
                    ToolTip.delay: 600
                    scale: pressed ? 0.98 : 1
                    contentItem: RowLayout {
                        spacing: 8
                        KIcon { Layout.leftMargin: 14; Layout.preferredWidth: 13; Layout.preferredHeight: 13; name: "plus"; color: KodosiTheme.accentInk; strokeWidth: 2.6 }
                        PlainLabel { Layout.fillWidth: true; text: qsTr("New terminal"); color: KodosiTheme.accentInk; font.weight: Font.DemiBold }
                    }
                    onClicked: root.newTerminalRequested("")
                }
                Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 18; color: KodosiTheme.alpha(KodosiTheme.accentInk, 0.2) }
                AbstractButton {
                    Layout.preferredWidth: 38
                    Layout.fillHeight: true
                    objectName: "sidebar.sessions.folder"
                    Accessible.id: objectName
                    Accessible.name: qsTr("New terminal in a folder")
                    activeFocusOnTab: true
                    hoverEnabled: true
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("New terminal in a folder…")
                    ToolTip.delay: 600
                    contentItem: Item {
                        KIcon { anchors.centerIn: parent; width: 14; height: 14; name: "folder"; color: KodosiTheme.accentInk; strokeWidth: 2 }
                    }
                    onClicked: Models.DesktopFiles.requestDirectory("new", Models.DesktopSettings.effectiveWorkingDirectory)
                }
            }
        }
        AbstractButton {
            id: needsYou
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            Layout.bottomMargin: 6
            implicitHeight: 30
            visible: root.waiting.length > 0
            objectName: "sidebar.needsYou"
            Accessible.id: objectName
            Accessible.name: label.text
            activeFocusOnTab: true
            scale: pressed ? 0.97 : 1
            contentItem: RowLayout {
                spacing: 9
                Item {
                    Layout.leftMargin: 8
                    Layout.preferredWidth: 22
                    Layout.preferredHeight: 22
                    BreathingDot { anchors.centerIn: parent }
                }
                PlainLabel {
                    id: label
                    Layout.fillWidth: true
                    text: root.waiting.length === 1 ? qsTr("%1 needs you").arg(root.waiting[0].name) : qsTr("%1 terminals need you").arg(root.waiting.length)
                    color: KodosiTheme.accentStrong
                    font.pixelSize: KodosiTheme.fontFootnote
                    font.weight: Font.Medium
                    elide: Text.ElideRight
                }
                KIcon { Layout.rightMargin: 10; Layout.preferredWidth: 11; Layout.preferredHeight: 11; name: "arrow-right"; color: KodosiTheme.accentStrong; strokeWidth: 2.4 }
            }
            background: Rectangle {
                radius: height / 2
                color: KodosiTheme.alpha(KodosiTheme.accentSoft, 0.8)
            }
            onClicked: root.openTerminal(root.waiting[0].id)
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
                        height: 78

                        Rectangle {
                            anchors.fill: parent
                            anchors.bottomMargin: 6
                            radius: KodosiTheme.radiusLg
                            color: KodosiTheme.alpha(KodosiTheme.accentSoft, 0.55)
                            border.width: 1
                            border.color: KodosiTheme.alpha(KodosiTheme.accent, 0.5)
                        }
                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 9
                            anchors.bottomMargin: 15
                            spacing: 7

                            RowLayout {
                                spacing: 9
                                RoomSigil { key: invitation.entry.missionId || ""; size: 22 }
                                PlainLabel {
                                    Layout.fillWidth: true
                                    text: invitation.entry.missionName || ""
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
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

                        width: parent ? parent.width : 0
                        height: 34
                        hoverEnabled: true
                        activeFocusOnTab: true
                        objectName: "sidebar.room." + entry.id
                        Accessible.id: objectName
                        Accessible.name: entry.name || ""
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
                                color: room.selected || room.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                font.weight: room.selected ? Font.DemiBold : Font.Medium
                                elide: Text.ElideRight
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
                                readonly property bool needsYou: Models.Sessions.attention.indexOf(sessionId) >= 0
                                readonly property bool renaming: root.renamingId === sessionId && sessionId.length > 0
                                readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)
                                readonly property string detail: session.activity || (session.kind === "remote" && session.connectionState !== "connected" ? (session.message || qsTr("Not connected")) : "")

                                width: parent ? parent.width : 0
                                height: detail.length > 0 ? 40 : 34
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
                                        asleep: !row.staged
                                        working: row.session.working === true
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 1
                                        visible: !row.renaming

                                        PlainLabel {
                                            Layout.fillWidth: true
                                            text: row.session.name || ""
                                            color: row.selected || rowHover.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                            font.weight: row.selected ? Font.DemiBold : Font.Medium
                                            elide: Text.ElideRight
                                        }
                                        ShimmerText {
                                            Layout.fillWidth: true
                                            visible: row.detail.length > 0
                                            text: row.detail
                                            active: row.session.working === true
                                            color: KodosiTheme.inkFaint
                                            font.pixelSize: KodosiTheme.fontCaption
                                            elide: Text.ElideRight
                                        }
                                    }
                                    KTextField {
                                        id: editor
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
                                    BreathingDot {
                                        Layout.rightMargin: 2
                                        visible: row.needsYou && !actions.visible
                                    }
                                    AvatarStack {
                                        visible: row.viewers.length > 0 && !actions.visible && !row.renaming
                                        userIds: row.viewers
                                        size: 18
                                        limit: 2
                                        ring: row.selected ? KodosiTheme.raised : KodosiTheme.ground
                                    }
                                    RoomSigil {
                                        visible: !!row.session.missionId && !actions.visible && !row.renaming
                                        key: row.session.missionId || ""
                                        size: 16
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
            NavRow {
                title: qsTr("Resume")
                iconName: "history"
                objectName: "sidebar.resume"
                onClicked: root.resumeRequested()
            }
            Item { width: 1; height: 4 }
            RowLayout {
                width: parent.width
                spacing: 6

                KButton {
                    Layout.fillWidth: true
                    visible: !root.accountReady
                    variant: Models.Account.signedIn ? KButton.Tinted : KButton.Secondary
                    text: Models.Account.signedIn ? qsTr("Approve this device") : Models.Account.signingIn ? qsTr("Signing in…") : qsTr("Sign in")
                    working: Models.Account.signingIn
                    objectName: "sidebar.signIn"
                    Accessible.id: objectName
                    onClicked: root.signIn()
                }
                KIconButton {
                    visible: !root.accountReady
                    size: 32
                    glyph: "settings"
                    active: root.selection === "settings"
                    objectName: "sidebar.settings.icon"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Settings")
                    onClicked: Models.DesktopState.activeView = Models.DesktopState.Settings
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
        KMenuItem { text: qsTr("Close…"); iconName: "close"; destructive: true; onTriggered: closeDialog.ask(menu.sessionId, menu.session.name || "") }
    }
    KDialog {
        id: closeDialog

        property string sessionId: ""
        property string sessionName: ""

        function ask(id, name) {
            sessionId = id
            sessionName = name
            open()
        }

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Close %1?").arg(sessionName)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Close")
        onAccepted: Models.SessionActions.close(sessionId)

        PlainLabel {
            width: 340
            text: qsTr("Its programs stop.")
            color: KodosiTheme.inkMuted
            wrapMode: Text.WordWrap
        }
    }
}
