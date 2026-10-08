pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQml.Models
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

Item {
    id: root
    readonly property var presentation: Models.Missions.presentation
    readonly property var room: Models.Missions.room
    readonly property var repositories: room.repositories || []
    property string repositoryFilter: ""
    function change(task, action, note) {
        let value = {
            type: "updateTask",
            taskId: task.id,
            change: action
        };
        if (note && note.trim().length)
            value.note = note;
        Models.Missions.roomAction(value);
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 16
            spacing: 10
            KCheckBox {
                text: qsTr("Done")
                checked: !!root.presentation.completed
                onToggled: Models.Missions.setPresentation("completed", checked)
            }
            PlainLabel {
                text: (root.room.tasks || []).filter(task => task.closed).length
                font.pixelSize: 11
                color: KodosiTheme.textSecondary
            }
            Item {
                Layout.fillWidth: true
            }
            KComboBox {
                model: [
                    {
                        id: "",
                        name: qsTr("All repositories")
                    }
                ].concat(root.repositories)
                textRole: "name"
                valueRole: "id"
                visible: root.repositories.length > 0
                Layout.maximumWidth: 190
                onActivated: root.repositoryFilter = currentValue
            }
            KButton {
                text: qsTr("New task")
                iconName: "plus"
                variant: KButton.Primary
                onClicked: Models.Missions.setPresentation("newTask", true)
                objectName: "room.task.new"
            }
        }
        ListView {
            id: taskList
            Layout.fillWidth: true
            Layout.fillHeight: true
            leftMargin: 16
            rightMargin: 16
            clip: true
            model: (root.room.tasks || []).filter(task => task.closed === !!root.presentation.completed && (!root.repositoryFilter || task.repositoryIds.indexOf(root.repositoryFilter) >= 0))
            ScrollBar.vertical: KScrollBar {}
            onMovementEnded: Models.Missions.setPresentation("taskY", contentY)
            onModelChanged: {
                const y = root.presentation.taskY || 0;
                Qt.callLater(function () {
                    taskList.contentY = y;
                });
            }
            delegate: ColumnLayout {
                id: row
                required property var modelData
                readonly property bool expanded: root.presentation.expandedTask === modelData.id
                width: taskList.width - 32
                spacing: 10
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 12
                    KIconButton {
                        glyph: row.modelData.closed ? "check" : "tasks"
                        Accessible.name: row.modelData.closed ? qsTr("Reopen %1").arg(row.modelData.title) : qsTr("Complete %1").arg(row.modelData.title)
                        enabled: !Models.Missions.busy
                        onClicked: root.change(row.modelData, row.modelData.closed ? "reopen" : "close", "")
                    }
                    ItemDelegate {
                        id: taskLabel
                        Accessible.name: row.modelData.title
                        Layout.fillWidth: true
                        implicitHeight: title.implicitHeight + 12
                        background: Rectangle {
                            color: taskLabel.hovered ? KodosiTheme.surface : "transparent"
                            radius: 8
                        }
                        contentItem: ColumnLayout {
                            id: title
                            spacing: 5
                            PlainLabel {
                                Layout.fillWidth: true
                                text: row.modelData.title
                                font.weight: Font.DemiBold
                                wrapMode: Text.WordWrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                            }
                            PlainLabel {
                                Layout.fillWidth: true
                                visible: row.modelData.repositoryIds.length > 0
                                text: root.repositories.filter(repo => row.modelData.repositoryIds.indexOf(repo.id) >= 0).map(repo => repo.name).join(" · ")
                                color: KodosiTheme.accent
                                font.pixelSize: 10
                                elide: Text.ElideRight
                            }
                        }
                        onClicked: Models.Missions.setPresentation("expandedTask", row.expanded ? "" : row.modelData.id)
                    }
                    RoomAvatar {
                        name: row.modelData.assignedName || ""
                        visible: !!row.modelData.assignedName
                        implicitWidth: 24
                        implicitHeight: 24
                    }
                    KIconButton {
                        glyph: "people"
                        visible: !row.modelData.assignedTo && !row.modelData.closed
                        Accessible.name: qsTr("Pick up %1").arg(row.modelData.title)
                        enabled: !Models.Missions.busy
                        onClicked: root.change(row.modelData, "claim", "")
                    }
                    KIcon {
                        name: row.expanded ? "chevron-up" : "chevron-down"
                        implicitWidth: 12
                        implicitHeight: 12
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 44
                    Layout.bottomMargin: 12
                    visible: row.expanded
                    spacing: 12
                    KReadOnlyText {
                        Layout.fillWidth: true
                        text: row.modelData.description
                        visible: text.length > 0
                    }
                    KButton {
                        visible: !!row.modelData.issue
                        text: qsTr("Open issue")
                        variant: KButton.Quiet
                        onClicked: {
                            const url = row.modelData.issue.url;
                            if (/^https?:\/\//.test(url))
                                Qt.openUrlExternally(url);
                        }
                    }
                    TextArea {
                        Layout.fillWidth: true
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        text: root.presentation["note." + row.modelData.id] ?? row.modelData.note ?? ""
                        onTextChanged: {
                            if (text !== (root.presentation["note." + row.modelData.id] ?? row.modelData.note ?? ""))
                                Models.Missions.setPresentation("note." + row.modelData.id, text);
                        }
                        placeholderText: qsTr("Result or pull request link")
                        color: KodosiTheme.textPrimary
                        placeholderTextColor: KodosiTheme.placeholderText
                        font.pixelSize: 13
                        background: Rectangle {
                            color: KodosiTheme.input
                            radius: 8
                            border.color: KodosiTheme.seam
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        KButton {
                            text: row.modelData.assignedTo ? qsTr("Release") : qsTr("Pick up")
                            visible: !row.modelData.closed
                            enabled: !Models.Missions.busy
                            onClicked: root.change(row.modelData, row.modelData.assignedTo ? "release" : "claim", "")
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        KButton {
                            text: row.modelData.closed ? qsTr("Save note") : qsTr("Complete")
                            variant: KButton.Primary
                            enabled: !Models.Missions.busy
                            onClicked: root.change(row.modelData, "close", root.presentation["note." + row.modelData.id])
                        }
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 1
                    color: KodosiTheme.seam
                    opacity: 0.55
                }
            }
            add: Transition {
                NumberAnimation {
                    properties: "opacity"
                    from: 0
                    to: 1
                    duration: KodosiTheme.motionFast
                }
            }
            remove: Transition {
                NumberAnimation {
                    properties: "opacity"
                    to: 0
                    duration: KodosiTheme.motionFast
                }
            }
            displaced: Transition {
                NumberAnimation {
                    properties: "y"
                    duration: KodosiTheme.motionFast
                    easing.type: Easing.OutCubic
                }
            }
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 16
                visible: taskList.count === 0
                KIcon {
                    name: root.presentation.completed ? "check" : "tasks"
                    implicitWidth: 44
                    implicitHeight: 44
                    Layout.alignment: Qt.AlignHCenter
                    color: KodosiTheme.accent
                }
                KButton {
                    text: qsTr("New task")
                    visible: !root.presentation.completed
                    onClicked: Models.Missions.setPresentation("newTask", true)
                }
            }
        }
    }
    KPopover {
        id: create
        parent: Overlay.overlay
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        width: 400
        padding: 20
        visible: !!root.presentation.newTask
        onClosed: Models.Missions.setPresentation("newTask", false)
        contentItem: ColumnLayout {
            spacing: 14
            KTextField {
                Layout.fillWidth: true
                text: root.presentation.taskTitle || ""
                onTextEdited: Models.Missions.setPresentation("taskTitle", text)
                placeholderText: qsTr("Task title")
                objectName: "room.task.title"
            }
            TextArea {
                Layout.fillWidth: true
                Layout.preferredHeight: 100
                text: root.presentation.taskDescription || ""
                onTextChanged: {
                    if (text !== (root.presentation.taskDescription || ""))
                        Models.Missions.setPresentation("taskDescription", text);
                }
                placeholderText: qsTr("Description")
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                color: KodosiTheme.textPrimary
                placeholderTextColor: KodosiTheme.placeholderText
                font.pixelSize: 13
                background: Rectangle {
                    color: KodosiTheme.input
                    radius: 8
                    border.color: KodosiTheme.seam
                }
                objectName: "room.task.description"
            }
            KButton {
                text: qsTr("Repositories")
                iconName: "repository"
                visible: root.repositories.length > 0
                onClicked: repoChoices.popup()
            }
            KMenu {
                id: repoChoices
                Instantiator {
                    model: root.repositories
                    onObjectAdded: (index, item) => repoChoices.insertItem(index, item)
                    onObjectRemoved: (index, item) => repoChoices.removeItem(item)
                    delegate: KMenuItem {
                        required property var modelData
                        text: modelData.name
                        checkable: true
                        checked: (root.presentation.taskRepositories || []).indexOf(modelData.id) >= 0
                        onTriggered: {
                            let selected = (root.presentation.taskRepositories || []).slice();
                            const index = selected.indexOf(modelData.id);
                            if (index >= 0)
                                selected.splice(index, 1);
                            else
                                selected.push(modelData.id);
                            Models.Missions.setPresentation("taskRepositories", selected);
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                KButton {
                    text: qsTr("Cancel")
                    onClicked: create.close()
                }
                Item {
                    Layout.fillWidth: true
                }
                KButton {
                    text: qsTr("Create task")
                    variant: KButton.Primary
                    enabled: !Models.Missions.busy && (root.presentation.taskTitle || "").trim().length > 0
                    onClicked: Models.Missions.roomAction({
                        type: "createTask",
                        title: root.presentation.taskTitle,
                        description: root.presentation.taskDescription || "",
                        repositoryIds: root.presentation.taskRepositories || []
                    })
                    objectName: "room.task.create"
                }
            }
        }
    }
}
