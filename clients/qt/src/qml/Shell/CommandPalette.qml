pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    readonly property var results: {
        const needle = query.text.trim().toLowerCase()
        const all = terminals().concat(rooms(), actions(), people())
        return needle.length === 0 ? all : all.filter(item => item.title.toLowerCase().indexOf(needle) >= 0 || (item.detail || "").toLowerCase().indexOf(needle) >= 0)
    }
    property int index: 0

    signal newTerminalRequested(string folder)
    signal newRoomRequested
    signal resumeRequested

    function terminals() {
        return Models.Sessions.sessions.map(session => ({
            id: "terminal." + session.id, group: qsTr("Terminals"), title: session.name, kind: "terminal", key: session.id,
            program: session.program, working: session.working, sign: session.sign, asks: session.asks, progress: session.progress,
            detail: [session.folderName, session.kind === "remote" ? session.hostLabel : ""].filter(part => !!part).join(" · ")
        }))
    }
    function rooms() {
        return Models.Missions.missions.map(room => ({ id: "room." + room.id, group: qsTr("Rooms"), title: room.name, kind: "room", key: room.id }))
    }
    function people() {
        return Models.People.friends.map(friend => ({
            id: "person." + friend.userId, group: qsTr("People"), title: friend.displayName || friend.handle,
            detail: "@" + friend.handle, kind: "person", key: friend.userId
        }))
    }
    function actions() {
        const group = qsTr("Actions")
        const list = [
            { id: "action.terminal", group: group, title: qsTr("New terminal"), kind: "action", icon: "plus", tint: "#d98a5f" },
            { id: "action.resume", group: group, title: qsTr("Resume a conversation"), kind: "action", icon: "history", tint: "#d9a441" }
        ]
        if (Models.Account.signedIn && Models.Devices.localDeviceEnrolled)
            list.push({ id: "action.room", group: group, title: qsTr("New room"), kind: "action", icon: "grid", tint: "#8c7bd1" })
        list.push({ id: "action.people", group: group, title: qsTr("People"), kind: "action", icon: "people", tint: "#5b82c9" })
        list.push({ id: "action.settings", group: group, title: qsTr("Settings"), kind: "action", icon: "settings", tint: "#6b6560" })
        return list
    }
    function run(item) {
        if (!item)
            return
        close()
        switch (item.kind) {
        case "terminal":
            Models.SessionActions.activate(item.key)
            break
        case "room":
            Models.DesktopState.activeView = Models.DesktopState.Missions
            Models.Missions.open(item.key)
            break
        case "person":
            Models.DesktopState.activeView = Models.DesktopState.People
            break
        default:
            if (item.id === "action.terminal") root.newTerminalRequested("")
            else if (item.id === "action.resume") root.resumeRequested()
            else if (item.id === "action.room") root.newRoomRequested()
            else if (item.id === "action.people") Models.DesktopState.activeView = Models.DesktopState.People
            else Models.DesktopState.activeView = Models.DesktopState.Settings
        }
    }
    function move(offset) {
        if (results.length === 0)
            return
        index = (index + offset + results.length) % results.length
        list.positionViewAtIndex(index, ListView.Contain)
    }

    objectName: "palette"
    modal: true
    focus: true
    parent: Overlay.overlay
    width: Math.min(580, parent ? parent.width - 48 : 580)
    x: parent ? (parent.width - width) / 2 : 0
    y: 96
    radius: KodosiTheme.radiusSheet
    elevation: 3
    fill: KodosiTheme.surface

    onOpened: {
        query.clear()
        index = 0
        query.forceActiveFocus()
    }

    contentItem: ColumnLayout {
        spacing: 0

        Well {
            Layout.fillWidth: true
            Layout.margins: 10
            Layout.preferredHeight: 38
            radius: KodosiTheme.radiusMd

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 10
                spacing: 10

                KIcon { Layout.preferredWidth: 15; Layout.preferredHeight: 15; name: "search"; color: KodosiTheme.inkMuted }
                TextInput {
                    id: query
                    Layout.fillWidth: true
                    color: KodosiTheme.ink
                    font.pixelSize: KodosiTheme.fontBody
                    selectionColor: KodosiTheme.accent
                    selectedTextColor: KodosiTheme.accentInk
                    clip: true
                    objectName: "palette.query"
                    Accessible.id: objectName
                    Accessible.name: placeholder.text
                    Accessible.role: Accessible.EditableText
                    onTextChanged: root.index = 0
                    Keys.onDownPressed: root.move(1)
                    Keys.onUpPressed: root.move(-1)
                    Keys.onReturnPressed: root.run(root.results[root.index])
                    Keys.onEnterPressed: root.run(root.results[root.index])

                    PlainLabel {
                        id: placeholder
                        visible: query.text.length === 0
                        text: qsTr("Go to a terminal, a room or a person")
                        color: KodosiTheme.inkFaint
                        font.pixelSize: KodosiTheme.fontBody
                    }
                }
                Keycap { text: "esc" }
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            Layout.topMargin: 14
            Layout.bottomMargin: 24
            visible: root.results.length === 0
            text: qsTr("Nothing matches.")
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
            horizontalAlignment: Text.AlignHCenter
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 10
            Layout.preferredHeight: Math.min(contentHeight, 380)
            visible: root.results.length > 0
            clip: true
            model: root.results
            spacing: 1
            ScrollBar.vertical: KScrollBar {}
            section.property: "group"
            section.delegate: PlainLabel {
                required property string section

                width: ListView.view.width
                height: 28
                leftPadding: 12
                text: section
                color: KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
                font.weight: Font.Medium
                verticalAlignment: Text.AlignBottom
                bottomPadding: 4
            }
            delegate: AbstractButton {
                id: entry

                required property var modelData
                required property int index
                readonly property bool highlighted: root.index === index

                width: ListView.view.width
                height: 36
                hoverEnabled: true
                objectName: "palette.item." + modelData.id
                Accessible.id: objectName
                Accessible.name: modelData.title

                contentItem: RowLayout {
                    spacing: 10

                    Item {
                        Layout.leftMargin: 8
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24

                        AgentMark { visible: entry.modelData.kind === "terminal"; program: entry.modelData.program || ""; size: 24; session: entry.modelData }
                        RoomSigil { visible: entry.modelData.kind === "room"; key: entry.modelData.key || ""; size: 24 }
                        PersonAvatar { visible: entry.modelData.kind === "person"; name: entry.modelData.title; key: entry.modelData.key || ""; size: 24 }
                        IconTile {
                            visible: entry.modelData.kind === "action"
                            iconName: entry.modelData.icon || "command"
                            tint: entry.modelData.tint || KodosiTheme.accent
                            size: 24
                        }
                    }
                    PlainLabel { text: entry.modelData.title; font.weight: Font.Medium; elide: Text.ElideRight; Layout.maximumWidth: 300 }
                    PlainLabel {
                        Layout.fillWidth: true
                        text: entry.modelData.detail || ""
                        color: KodosiTheme.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                        elide: Text.ElideRight
                    }
                    Keycap { Layout.rightMargin: 8; visible: entry.highlighted; text: "⏎" }
                }
                background: Item {
                    Raised {
                        anchors.fill: parent
                        radius: KodosiTheme.radiusMd
                        visible: entry.highlighted
                    }
                    Rectangle {
                        anchors.fill: parent
                        radius: KodosiTheme.radiusMd
                        color: KodosiTheme.alpha(KodosiTheme.ink, !entry.highlighted && entry.hovered ? 0.05 : 0)
                    }
                    Rectangle {
                        x: -1.5
                        anchors.verticalCenter: parent.verticalCenter
                        width: 3
                        height: 14
                        radius: 1.5
                        visible: entry.highlighted
                        color: KodosiTheme.accent
                    }
                }

                onClicked: root.run(modelData)
            }
        }
    }

    component Keycap: Item {
        id: cap

        property string text: ""

        implicitWidth: Math.max(20, label.implicitWidth + 12)
        implicitHeight: 18

        Bezel {
            anchors.fill: parent
            radius: KodosiTheme.radiusXs
            quiet: true
            fill: KodosiTheme.lifted
        }
        PlainLabel {
            id: label
            anchors.centerIn: parent
            text: cap.text
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontCaption2
        }
    }
}
