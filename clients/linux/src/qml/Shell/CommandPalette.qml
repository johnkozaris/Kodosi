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
            program: session.program, working: session.working,
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
            { id: "action.terminal", group: group, title: qsTr("New terminal"), kind: "action", icon: "plus" },
            { id: "action.folder", group: group, title: qsTr("New terminal in a folder…"), kind: "action", icon: "folder" },
            { id: "action.resume", group: group, title: qsTr("Resume a conversation"), kind: "action", icon: "history" }
        ]
        if (Models.Account.signedIn && Models.Devices.localDeviceEnrolled)
            list.push({ id: "action.room", group: group, title: qsTr("New room"), kind: "action", icon: "grid" })
        list.push({ id: "action.people", group: group, title: qsTr("People"), kind: "action", icon: "people" })
        list.push({ id: "action.settings", group: group, title: qsTr("Settings"), kind: "action", icon: "settings" })
        return list
    }
    function run(item) {
        if (!item)
            return
        close()
        switch (item.kind) {
        case "terminal":
            Models.Sessions.clearAttention(item.key)
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
            else if (item.id === "action.folder") Models.DesktopFiles.requestDirectory("new", Models.DesktopSettings.effectiveWorkingDirectory)
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

    onOpened: {
        query.clear()
        index = 0
        query.forceActiveFocus()
    }

    contentItem: ColumnLayout {
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 18
            Layout.preferredHeight: 54
            spacing: 10

            KIcon { Layout.preferredWidth: 17; Layout.preferredHeight: 17; name: "search"; color: KodosiTheme.inkFaint }
            TextInput {
                id: query
                Layout.fillWidth: true
                color: KodosiTheme.ink
                font.pixelSize: KodosiTheme.fontCallout
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
                    font.pixelSize: KodosiTheme.fontCallout
                }
            }
            Well {
                Layout.preferredWidth: 30
                Layout.preferredHeight: 18
                radius: KodosiTheme.radiusXs
                PlainLabel { anchors.centerIn: parent; text: "esc"; color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption2 }
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: KodosiTheme.alpha(KodosiTheme.hairline, 0.7) }
        PlainLabel {
            Layout.fillWidth: true
            Layout.topMargin: 26
            Layout.bottomMargin: 26
            visible: root.results.length === 0
            text: qsTr("Nothing matches.")
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
            horizontalAlignment: Text.AlignHCenter
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.margins: 8
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
                height: 38
                hoverEnabled: true
                objectName: "palette.item." + modelData.id
                Accessible.id: objectName
                Accessible.name: modelData.title

                contentItem: RowLayout {
                    spacing: 11

                    Item {
                        Layout.leftMargin: 10
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24

                        AgentMark { visible: entry.modelData.kind === "terminal"; program: entry.modelData.program || ""; size: 24; working: entry.modelData.working === true }
                        RoomSigil { visible: entry.modelData.kind === "room"; key: entry.modelData.key || ""; size: 24 }
                        PersonAvatar { visible: entry.modelData.kind === "person"; name: entry.modelData.title; key: entry.modelData.key || ""; size: 24 }
                        Well {
                            visible: entry.modelData.kind === "action"
                            anchors.fill: parent
                            radius: KodosiTheme.radiusSm
                            KIcon { anchors.centerIn: parent; width: 13; height: 13; name: entry.modelData.icon || "command"; strokeWidth: 2 }
                        }
                    }
                    PlainLabel { text: entry.modelData.title; font.weight: Font.Medium; elide: Text.ElideRight; Layout.maximumWidth: 300 }
                    PlainLabel {
                        Layout.fillWidth: true
                        text: entry.modelData.detail || ""
                        color: KodosiTheme.inkFaint
                        font.pixelSize: KodosiTheme.fontFootnote
                        elide: Text.ElideRight
                    }
                    KIcon { Layout.rightMargin: 12; Layout.preferredWidth: 12; Layout.preferredHeight: 12; visible: entry.highlighted; name: "arrow-right"; color: KodosiTheme.inkFaint; strokeWidth: 2.2 }
                }
                background: Rectangle {
                    radius: KodosiTheme.radiusMd
                    color: entry.highlighted ? KodosiTheme.accentSoft : KodosiTheme.alpha(KodosiTheme.ink, entry.hovered ? 0.05 : 0)
                }

                onClicked: root.run(modelData)
            }
        }
    }
}
