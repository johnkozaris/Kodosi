pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property var presentation: Models.Missions.presentation
    readonly property var room: Models.Missions.room
    readonly property var repositories: room.repositories || []
    readonly property var tasks: room.tasks || []
    readonly property var lanes: [
        { key: "open", title: qsTr("Up for grabs"), tasks: tasks.filter(task => !task.closed && !task.assignedTo) },
        { key: "progress", title: qsTr("In progress"), tasks: tasks.filter(task => !task.closed && !!task.assignedTo) },
        { key: "done", title: qsTr("Done"), tasks: tasks.filter(task => task.closed) }
    ]

    function change(task, action, note) {
        let value = { type: "updateTask", taskId: task.id, change: action };
        if (note && note.trim().length)
            value.note = note;
        Models.Missions.roomAction(value);
    }
    function add() {
        const title = (root.presentation.taskTitle || "").trim();
        if (title.length === 0 || Models.Missions.busy)
            return;
        Models.Missions.roomAction({ type: "createTask", title: title, description: "", repositoryIds: [] });
    }

    objectName: "room.tasks"
    Accessible.id: objectName

    KScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            x: 14
            width: Math.min(760, scroll.availableWidth - 28)
            spacing: 8

            Item {
                Layout.fillWidth: true
                Layout.topMargin: 2
                implicitHeight: 42

                Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; elevation: quick.activeFocus ? 2 : 1 }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 14
                    anchors.rightMargin: 8
                    spacing: 10

                    KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "plus"; strokeWidth: 2.2; color: quick.activeFocus ? KodosiTheme.accentStrong : KodosiTheme.inkFaint }
                    TextInput {
                        id: quick
                        Layout.fillWidth: true
                        text: root.presentation.taskTitle || ""
                        onTextEdited: Models.Missions.setPresentation("taskTitle", text)
                        color: KodosiTheme.ink
                        font.pixelSize: KodosiTheme.fontBody
                        selectionColor: KodosiTheme.accent
                        selectedTextColor: KodosiTheme.accentInk
                        clip: true
                        objectName: "room.task.title"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Add a task")
                        Accessible.role: Accessible.EditableText
                        onAccepted: root.add()

                        PlainLabel { visible: quick.text.length === 0; text: qsTr("Add a task"); color: KodosiTheme.inkFaint }
                    }
                    KButton {
                        compact: true
                        variant: KButton.Primary
                        visible: quick.text.trim().length > 0
                        working: Models.Missions.busy
                        text: qsTr("Add")
                        objectName: "room.task.create"
                        Accessible.id: objectName
                        onClicked: root.add()
                    }
                }
            }
            Repeater {
                model: root.lanes

                ColumnLayout {
                    id: lane

                    required property var modelData
                    readonly property bool done: modelData.key === "done"
                    readonly property bool open: !done || !!root.presentation.completed

                    Layout.fillWidth: true
                    visible: modelData.tasks.length > 0
                    spacing: 8

                    AbstractButton {
                        Layout.topMargin: 10
                        Layout.leftMargin: 4
                        enabled: lane.done
                        Accessible.name: lane.modelData.title
                        objectName: "room.tasks.lane." + lane.modelData.key
                        Accessible.id: objectName
                        contentItem: RowLayout {
                            spacing: 6
                            KIcon { visible: lane.done; Layout.preferredWidth: 9; Layout.preferredHeight: 9; name: "chevron-down"; strokeWidth: 2.6; color: KodosiTheme.inkFaint; rotation: lane.open ? 0 : -90 }
                            PlainLabel { text: lane.modelData.title; color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote; font.weight: Font.DemiBold }
                            PlainLabel { text: lane.modelData.tasks.length; color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontFootnote }
                        }
                        onClicked: Models.Missions.setPresentation("completed", !root.presentation.completed)
                    }
                    Repeater {
                        model: lane.open ? lane.modelData.tasks : []

                        Item {
                            id: card

                            required property var modelData
                            readonly property bool expanded: root.presentation.expandedTask === modelData.id
                            readonly property string note: root.presentation["note." + modelData.id] ?? modelData.note ?? ""

                            Layout.fillWidth: true
                            implicitHeight: column.implicitHeight + 20

                            Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; fill: hover.hovered ? KodosiTheme.lifted : KodosiTheme.raised }
                            HoverHandler { id: hover }
                            ColumnLayout {
                                id: column
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.top: parent.top
                                anchors.margins: 10
                                anchors.leftMargin: 12
                                spacing: 10

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 10

                                    AbstractButton {
                                        id: check
                                        Layout.alignment: Qt.AlignTop
                                        implicitWidth: 22
                                        implicitHeight: 22
                                        hoverEnabled: true
                                        enabled: !Models.Missions.busy
                                        Accessible.name: card.modelData.closed ? qsTr("Reopen %1").arg(card.modelData.title) : qsTr("Complete %1").arg(card.modelData.title)
                                        contentItem: Item {
                                            Rectangle {
                                                anchors.centerIn: parent
                                                width: 18
                                                height: 18
                                                radius: 9
                                                color: card.modelData.closed ? KodosiTheme.ready : "transparent"
                                                border.width: 1.5
                                                border.color: card.modelData.closed ? KodosiTheme.ready : check.hovered ? KodosiTheme.accent : KodosiTheme.inkFaint
                                                KIcon { anchors.centerIn: parent; width: 10; height: 10; name: "check"; strokeWidth: 3; visible: card.modelData.closed || check.hovered; color: card.modelData.closed ? KodosiTheme.surface : KodosiTheme.accent }
                                            }
                                        }
                                        onClicked: root.change(card.modelData, card.modelData.closed ? "reopen" : "close", "")
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 4

                                        PlainLabel {
                                            Layout.fillWidth: true
                                            text: card.modelData.title
                                            color: card.modelData.closed ? KodosiTheme.inkMuted : KodosiTheme.ink
                                            font.weight: Font.DemiBold
                                            font.strikeout: card.modelData.closed
                                            wrapMode: Text.WordWrap
                                            maximumLineCount: card.expanded ? 8 : 2
                                            elide: Text.ElideRight

                                            TapHandler { onTapped: Models.Missions.setPresentation("expandedTask", card.expanded ? "" : card.modelData.id) }
                                        }
                                        Flow {
                                            Layout.fillWidth: true
                                            visible: card.modelData.repositoryIds.length > 0 || !!card.modelData.issue
                                            spacing: 5
                                            Repeater {
                                                model: root.repositories.filter(repo => card.modelData.repositoryIds.indexOf(repo.id) >= 0)
                                                Tag { required property var modelData; text: modelData.name; iconName: "repository" }
                                            }
                                            Tag { visible: !!card.modelData.issue; text: card.modelData.issue ? "#" + card.modelData.issue.number : ""; tone: Tag.Accent }
                                        }
                                    }
                                    PersonAvatar {
                                        Layout.alignment: Qt.AlignTop
                                        visible: !!card.modelData.assignedName
                                        name: card.modelData.assignedName || ""
                                        key: card.modelData.assignedTo || ""
                                        isSelf: card.modelData.assignedTo === Models.Account.userId
                                        size: 22
                                    }
                                    KButton {
                                        Layout.alignment: Qt.AlignTop
                                        compact: true
                                        variant: KButton.Tinted
                                        visible: !card.modelData.assignedTo && !card.modelData.closed && (hover.hovered || card.expanded)
                                        enabled: !Models.Missions.busy
                                        text: qsTr("Pick up")
                                        Accessible.name: qsTr("Pick up %1").arg(card.modelData.title)
                                        onClicked: root.change(card.modelData, "claim", "")
                                    }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Layout.leftMargin: 32
                                    visible: card.expanded
                                    spacing: 10

                                    KReadOnlyText {
                                        Layout.fillWidth: true
                                        text: card.modelData.description
                                        color: KodosiTheme.inkMuted
                                        visible: text.length > 0
                                    }
                                    Well {
                                        Layout.fillWidth: true
                                        implicitHeight: Math.max(54, note.implicitHeight + 4)
                                        radius: KodosiTheme.radiusMd

                                        TextArea {
                                            id: note
                                            anchors.fill: parent
                                            wrapMode: TextEdit.Wrap
                                            selectByMouse: true
                                            text: card.note
                                            onTextChanged: {
                                                if (text !== card.note)
                                                    Models.Missions.setPresentation("note." + card.modelData.id, text);
                                            }
                                            placeholderText: qsTr("Result or pull request link")
                                            color: KodosiTheme.ink
                                            placeholderTextColor: KodosiTheme.inkFaint
                                            font.pixelSize: KodosiTheme.fontBody
                                            background: null
                                        }
                                    }
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 8

                                        KButton {
                                            compact: true
                                            variant: KButton.Ghost
                                            visible: !!card.modelData.issue
                                            iconName: "link"
                                            text: qsTr("Open issue")
                                            onClicked: {
                                                const url = card.modelData.issue.url;
                                                if (/^https?:\/\//.test(url))
                                                    Qt.openUrlExternally(url);
                                            }
                                        }
                                        KButton {
                                            compact: true
                                            variant: KButton.Ghost
                                            text: qsTr("Release")
                                            visible: !card.modelData.closed && !!card.modelData.assignedTo
                                            enabled: !Models.Missions.busy
                                            onClicked: root.change(card.modelData, "release", "")
                                        }
                                        Item { Layout.fillWidth: true }
                                        KButton {
                                            compact: true
                                            variant: KButton.Primary
                                            text: card.modelData.closed ? qsTr("Save note") : qsTr("Complete")
                                            enabled: !Models.Missions.busy
                                            onClicked: root.change(card.modelData, "close", card.note)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            EmptyState {
                Layout.fillWidth: true
                Layout.topMargin: 60
                visible: root.tasks.length === 0
                title: qsTr("No tasks yet")
                message: qsTr("Write one above. People and agents can pick it up.")
                art: IconTile { iconName: "tasks"; tint: "#4f9bb0"; size: 48 }
            }
            Item { Layout.preferredHeight: 20 }
        }
    }
}
