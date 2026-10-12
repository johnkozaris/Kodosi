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
    readonly property var issues: Models.Missions.issueRepository === selected ? Models.Missions.issues : []
    property string requested: ""

    function select(id) {
        Models.Missions.setPresentation("repository", id);
        requested = id;
        Models.Missions.roomAction({ type: "issues", repositoryId: id });
    }
    function connect() {
        const url = (root.presentation.repositoryUrl || "").trim();
        if (url.length > 0 && !Models.Missions.busy)
            Models.Missions.roomAction({ type: "addRepository", url: url });
    }

    objectName: "room.repositories"
    Accessible.id: objectName
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

    component ConnectField: Item {
        id: field

        property alias text: url.text
        property bool busy: false

        signal edited(string text)
        signal submitted

        implicitHeight: 40
        implicitWidth: 420

        Well { anchors.fill: parent; radius: 20 }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 5
            spacing: 8

            KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "link"; color: KodosiTheme.inkFaint }
            TextInput {
                id: url
                Layout.fillWidth: true
                color: KodosiTheme.ink
                font.pixelSize: KodosiTheme.fontBody
                selectionColor: KodosiTheme.accent
                selectedTextColor: KodosiTheme.accentInk
                clip: true
                objectName: "room.repository.url"
                Accessible.id: objectName
                Accessible.name: qsTr("Repository address")
                Accessible.role: Accessible.EditableText
                onTextEdited: field.edited(text)
                onAccepted: field.submitted()

                PlainLabel { visible: url.text.length === 0; text: qsTr("Paste a repository address"); color: KodosiTheme.inkFaint }
            }
            KButton {
                compact: true
                variant: KButton.Primary
                text: qsTr("Connect")
                working: field.busy
                enabled: url.text.trim().length > 0 && !field.busy
                objectName: "room.repository.add"
                Accessible.id: objectName
                onClicked: field.submitted()
            }
        }
    }

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(400, parent.width - 40)
        visible: root.repositories.length === 0
        title: qsTr("Connect a repository")
        message: qsTr("Its issues can become room tasks. Your sign-in for it stays on this computer.")
        art: IconTile { iconName: "mission"; tint: "#8c7bd1"; size: 48 }

        ConnectField {
            text: root.presentation.repositoryUrl || ""
            busy: Models.Missions.busy
            onEdited: text => Models.Missions.setPresentation("repositoryUrl", text)
            onSubmitted: root.connect()
        }
    }
    ColumnLayout {
        anchors.fill: parent
        visible: root.repositories.length > 0
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            spacing: 8

            Flickable {
                Layout.fillWidth: true
                implicitHeight: 30
                contentWidth: chips.implicitWidth
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Row {
                    id: chips
                    spacing: 6

                    Repeater {
                        model: root.repositories

                        AbstractButton {
                            id: chip

                            required property var modelData
                            readonly property bool current: root.selected === modelData.id

                            height: 30
                            implicitWidth: chipRow.implicitWidth + 22
                            hoverEnabled: true
                            Accessible.name: modelData.name
                            Accessible.selected: current
                            objectName: "room.repository." + modelData.id
                            Accessible.id: objectName
                            contentItem: Item {
                                RowLayout {
                                    id: chipRow
                                    x: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 6
                                    KIcon { Layout.preferredWidth: 12; Layout.preferredHeight: 12; name: "repository"; color: chip.current ? KodosiTheme.accentStrong : KodosiTheme.inkMuted }
                                    PlainLabel { text: chip.modelData.name; color: chip.current ? KodosiTheme.ink : KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote; font.weight: chip.current ? Font.DemiBold : Font.Medium }
                                }
                            }
                            background: Item {
                                Raised { anchors.fill: parent; radius: 15; visible: chip.current }
                                Rectangle { anchors.fill: parent; radius: 15; visible: !chip.current; color: KodosiTheme.alpha(KodosiTheme.ink, chip.hovered ? 0.08 : 0.04) }
                            }
                            onClicked: root.select(modelData.id)
                        }
                    }
                }
            }
            KButton {
                compact: true
                variant: KButton.Ghost
                iconName: "plus"
                text: qsTr("Connect")
                objectName: "room.repository.new"
                Accessible.id: objectName
                onClicked: Models.Missions.setPresentation("newRepository", true)
            }
        }
        RoomSkeleton {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Models.Missions.busy && root.issues.length === 0
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !(Models.Missions.busy && root.issues.length === 0)
            leftMargin: 14
            rightMargin: 14
            bottomMargin: 14
            spacing: 6
            clip: true
            model: root.issues
            ScrollBar.vertical: KScrollBar {}
            delegate: Item {
                id: entry

                required property var modelData
                readonly property bool imported: (Models.Missions.room.tasks || []).some(task => task.issue && task.issue.url === modelData.url)

                width: Math.min(760, list.width - 28)
                height: 52

                Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 8
                    spacing: 10

                    Tag { text: "#" + entry.modelData.number; tone: entry.modelData.closed ? Tag.Ready : Tag.Neutral }
                    PlainLabel { Layout.fillWidth: true; text: entry.modelData.title; font.weight: Font.Medium; elide: Text.ElideRight }
                    KButton {
                        compact: true
                        variant: entry.imported ? KButton.Ghost : KButton.Tinted
                        iconName: entry.imported ? "check" : "plus"
                        text: entry.imported ? qsTr("In tasks") : qsTr("Make a task")
                        enabled: !entry.imported && !Models.Missions.busy
                        Accessible.name: entry.imported ? qsTr("Added to tasks") : qsTr("Add %1 to tasks").arg(entry.modelData.title)
                        onClicked: Models.Missions.roomAction({ type: "importIssue", repositoryId: entry.modelData.repositoryId, number: entry.modelData.number })
                    }
                }
            }

            EmptyState {
                anchors.centerIn: parent
                width: Math.min(340, parent.width - 40)
                visible: list.count === 0
                title: root.presentation.failure ? qsTr("The issues did not load") : qsTr("No open issues")
                message: root.presentation.failure || ""

                KButton { compact: true; iconName: "refresh"; text: qsTr("Refresh"); onClicked: root.select(root.selected || root.repositories[0].id) }
            }
        }
    }
    KPopover {
        id: connectRepository
        modal: true
        focus: true
        parent: Overlay.overlay
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        width: 460
        padding: 20
        radius: KodosiTheme.radiusSheet
        elevation: 3
        visible: !!root.presentation.newRepository && root.repositories.length > 0
        onClosed: Models.Missions.setPresentation("newRepository", false)
        contentItem: ColumnLayout {
            spacing: 14
            PlainLabel { text: qsTr("Connect a repository"); font.pixelSize: KodosiTheme.fontTitle; font.weight: Font.DemiBold }
            ConnectField {
                Layout.fillWidth: true
                text: root.presentation.repositoryUrl || ""
                busy: Models.Missions.busy
                onEdited: text => Models.Missions.setPresentation("repositoryUrl", text)
                onSubmitted: root.connect()
            }
        }
    }
}
