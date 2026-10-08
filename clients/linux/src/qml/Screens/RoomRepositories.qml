pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

Item {
    id: root
    readonly property var presentation: Models.Missions.presentation
    readonly property var repositories: Models.Missions.room.repositories || []
    readonly property string selected: presentation.repository || ""
    property string requested: ""
    function select(id) {
        Models.Missions.setPresentation("repository", id);
        requested = id;
        Models.Missions.roomAction({
            type: "issues",
            repositoryId: id
        });
    }
    onRepositoriesChanged: {
        if (visible && !selected && repositories.length)
            select(repositories[0].id);
    }
    Component.onCompleted: {
        if (!visible)
            return;
        if (selected)
            select(selected);
        else if (repositories.length)
            select(repositories[0].id);
    }
    onVisibleChanged: {
        if (!visible)
            return;
        const repository = selected || (repositories.length ? repositories[0].id : "");
        if (repository && requested !== repository)
            select(repository);
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 16
            PlainLabel {
                Layout.fillWidth: true
                text: (root.repositories.find(repo => repo.id === root.selected) || {}).name || ""
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            KButton {
                text: qsTr("Connect repository")
                iconName: "plus"
                variant: KButton.Primary
                onClicked: Models.Missions.setPresentation("newRepository", true)
                objectName: "room.repository.new"
            }
        }
        Flickable {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            contentWidth: repositoryTabs.implicitWidth
            clip: true
            Row {
                id: repositoryTabs
                spacing: 8
                leftPadding: 16
                rightPadding: 16
                Repeater {
                    model: root.repositories
                    delegate: KButton {
                        required property var modelData
                        text: modelData.name
                        iconName: "repository"
                        variant: root.selected === modelData.id ? KButton.Secondary : KButton.Quiet
                        onClicked: root.select(modelData.id)
                    }
                }
            }
        }
        RoomSkeleton {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Models.Missions.busy && root.repositories.length > 0
        }
        ListView {
            id: issues
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !Models.Missions.busy
            leftMargin: 18
            rightMargin: 18
            clip: true
            model: Models.Missions.issueRepository === root.selected ? Models.Missions.issues : []
            ScrollBar.vertical: KScrollBar {}
            delegate: ColumnLayout {
                id: entry
                required property var modelData
                readonly property bool imported: (Models.Missions.room.tasks || []).some(task => task.issue && task.issue.url === modelData.url)
                width: issues.width - 36
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 14
                    Layout.bottomMargin: 14
                    spacing: 12
                    KIcon {
                        name: entry.modelData.closed ? "check" : "tasks"
                        color: KodosiTheme.accent
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 5
                        PlainLabel {
                            Layout.fillWidth: true
                            text: entry.modelData.title
                            font.weight: Font.DemiBold
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                        PlainLabel {
                            text: "#" + entry.modelData.number
                            font.pixelSize: 11
                            color: KodosiTheme.textSecondary
                        }
                    }
                    KIconButton {
                        glyph: entry.imported ? "check" : "plus"
                        enabled: !entry.imported && !Models.Missions.busy
                        Accessible.name: entry.imported ? qsTr("Added to tasks") : qsTr("Add %1 to tasks").arg(entry.modelData.title)
                        onClicked: Models.Missions.roomAction({
                            type: "importIssue",
                            repositoryId: entry.modelData.repositoryId,
                            number: entry.modelData.number
                        })
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 1
                    color: KodosiTheme.seam
                    opacity: 0.55
                }
            }
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 16
                visible: issues.count === 0
                KIcon {
                    name: root.presentation.failure ? "warning" : "repository"
                    implicitWidth: 44
                    implicitHeight: 44
                    Layout.alignment: Qt.AlignHCenter
                    color: KodosiTheme.accent
                }
                KButton {
                    text: root.repositories.length ? qsTr("Refresh") : qsTr("Connect repository")
                    onClicked: {
                        if (root.repositories.length)
                            root.select(root.selected || root.repositories[0].id);
                        else
                            Models.Missions.setPresentation("newRepository", true);
                    }
                }
            }
        }
    }
    KPopover {
        id: connectRepository
        parent: Overlay.overlay
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        width: 400
        padding: 20
        visible: !!root.presentation.newRepository
        onClosed: Models.Missions.setPresentation("newRepository", false)
        contentItem: ColumnLayout {
            spacing: 14
            KTextField {
                Layout.fillWidth: true
                text: root.presentation.repositoryUrl || ""
                onTextEdited: Models.Missions.setPresentation("repositoryUrl", text)
                placeholderText: qsTr("Repository URL")
                objectName: "room.repository.url"
            }
            RowLayout {
                Layout.fillWidth: true
                KButton {
                    text: qsTr("Cancel")
                    onClicked: connectRepository.close()
                }
                Item {
                    Layout.fillWidth: true
                }
                KButton {
                    text: qsTr("Connect")
                    variant: KButton.Primary
                    enabled: !Models.Missions.busy && (root.presentation.repositoryUrl || "").trim().length > 0
                    onClicked: Models.Missions.roomAction({
                        type: "addRepository",
                        url: root.presentation.repositoryUrl
                    })
                    objectName: "room.repository.add"
                }
            }
        }
    }
}
