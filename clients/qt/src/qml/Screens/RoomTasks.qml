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
    readonly property string repositoryFilter: repositories.some(repository => repository.id === presentation.taskFilter) ? presentation.taskFilter : ""
    readonly property var tasks: (room.tasks || []).filter(task => repositoryFilter.length === 0 || task.repositoryIds.indexOf(repositoryFilter) >= 0)
    readonly property var chosen: presentation.taskRepositories || []
    readonly property bool adding: composer.activeFocus || (presentation.taskDescription || "").length > 0 || chosen.length > 0

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
        Models.Missions.roomAction({ type: "createTask", title: title, description: (root.presentation.taskDescription || "").trim(),
            repositoryIds: root.chosen.filter(id => root.repositories.some(repository => repository.id === id)) });
    }
    function toggle(id) {
        const next = root.chosen.indexOf(id) >= 0 ? root.chosen.filter(entry => entry !== id) : root.chosen.concat([id]);
        Models.Missions.setPresentation("taskRepositories", next);
    }

    objectName: "room.tasks"
    Accessible.id: objectName

    component Lane: ColumnLayout {
        id: lane

        required property string key
        required property string title
        required property var values
        property bool folds: false
        readonly property bool open: !folds || !!root.presentation.completed

        Layout.fillWidth: true
        Layout.topMargin: 10
        visible: cards.count > 0
        spacing: 8

        Models.KeyedList {
            id: cards
            values: lane.values
        }
        AbstractButton {
            Layout.leftMargin: 4
            enabled: lane.folds
            Accessible.name: lane.title
            objectName: "room.tasks.lane." + lane.key
            Accessible.id: objectName
            contentItem: RowLayout {
                spacing: 6
                KIcon {
                    visible: lane.folds
                    Layout.preferredWidth: 9
                    Layout.preferredHeight: 9
                    name: "chevron-down"
                    strokeWidth: 2.6
                    color: KodosiTheme.inkFaint
                    rotation: lane.open ? 0 : -90

                    Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                }
                PlainLabel { text: lane.title; color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote; font.weight: Font.DemiBold }
                RollingText { text: String(cards.count); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontFootnote }
            }
            onClicked: Models.Missions.setPresentation("completed", !root.presentation.completed)
        }
        Repeater {
            model: lane.open ? cards : null

            TaskCard {}
        }
    }

    component TaskCard: Item {
        id: card

        required property var modelData
        readonly property bool expanded: root.presentation.expandedTask === modelData.id
        readonly property string note: root.presentation["note." + modelData.id] ?? modelData.note ?? ""
        readonly property bool mine: !!modelData.assignedTo && modelData.assignedTo === Models.Account.userId
        readonly property var terminal: modelData.terminalId ? Models.Sessions.sessions.find(session => session.id === modelData.terminalId) || null : null
        readonly property var version: modelData.version
        property bool ready: false

        Layout.fillWidth: true
        implicitHeight: column.implicitHeight + 24
        objectName: "room.task." + modelData.id
        Accessible.id: objectName

        Component.onCompleted: ready = true
        onVersionChanged: {
            if (ready && !KodosiTheme.reduceMotion)
                wash.restart();
        }

        Raised {
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
            fill: hover.hovered && !card.expanded ? KodosiTheme.lifted : KodosiTheme.raised
            elevation: card.expanded ? 2 : 1
        }
        Rectangle {
            id: glow
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
            color: KodosiTheme.glowAmber
            opacity: 0
            visible: opacity > 0
        }
        SequentialAnimation {
            id: wash
            NumberAnimation { target: glow; property: "opacity"; to: 0.3; duration: 180; easing.type: Easing.OutQuad }
            NumberAnimation { target: glow; property: "opacity"; to: 0; duration: 2400; easing.type: Easing.OutQuad }
        }
        Rectangle {
            x: -1
            y: 12
            width: 3
            height: parent.height - 24
            radius: 1.5
            visible: card.mine && !card.modelData.closed
            color: KodosiTheme.accent
        }
        HoverHandler { id: hover }
        ColumnLayout {
            id: column
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            anchors.leftMargin: 14
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                AbstractButton {
                    id: check
                    Layout.alignment: Qt.AlignTop
                    implicitWidth: 22
                    implicitHeight: 22
                    hoverEnabled: true
                    enabled: !Models.Missions.busy
                    scale: pressed ? 0.95 : 1
                    Accessible.name: card.modelData.closed ? qsTr("Reopen %1").arg(card.modelData.title) : qsTr("Complete %1").arg(card.modelData.title)
                    ToolTip.visible: hovered
                    ToolTip.text: card.modelData.closed ? qsTr("Reopen") : qsTr("Complete")
                    ToolTip.delay: Tip.delay

                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

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
                    spacing: 6

                    PlainLabel {
                        Layout.fillWidth: true
                        text: card.modelData.title
                        color: card.modelData.closed ? KodosiTheme.inkMuted : KodosiTheme.ink
                        font.weight: Font.Medium
                        font.strikeout: card.modelData.closed
                        wrapMode: Text.WordWrap
                        maximumLineCount: card.expanded ? 8 : 2
                        elide: Text.ElideRight

                        TapHandler { onTapped: Models.Missions.setPresentation("expandedTask", card.expanded ? "" : card.modelData.id) }
                    }
                    Flow {
                        Layout.fillWidth: true
                        visible: card.modelData.repositoryIds.length > 0 || !!card.modelData.issue || card.terminal !== null
                            || (card.modelData.closed && !!card.modelData.note)
                        spacing: 5

                        Repeater {
                            model: root.repositories.filter(repository => card.modelData.repositoryIds.indexOf(repository.id) >= 0)

                            Tag {
                                required property var modelData

                                text: modelData.name
                                iconName: "repository"
                            }
                        }
                        Tag { visible: !!card.modelData.issue; text: card.modelData.issue ? "#" + card.modelData.issue.number : ""; iconName: "link" }
                        Tag {
                            visible: card.terminal !== null
                            text: card.terminal ? card.terminal.name : ""
                            iconName: "terminal"
                            tone: card.terminal && card.terminal.working ? Tag.Accent : Tag.Neutral
                        }
                        Tag {
                            visible: card.modelData.closed && !!card.modelData.note
                            text: card.modelData.note || ""
                            iconName: "chat"
                            tone: Tag.Ready
                        }
                    }
                }
                PersonAvatar {
                    Layout.alignment: Qt.AlignTop
                    visible: !!card.modelData.assignedName
                    name: card.modelData.assignedName || ""
                    key: card.modelData.assignedTo || ""
                    isSelf: card.mine
                    size: 24
                }
                KButton {
                    Layout.alignment: Qt.AlignTop
                    compact: true
                    variant: KButton.Tinted
                    visible: !card.modelData.assignedName && !card.modelData.closed
                    opacity: hover.hovered || card.expanded || visualFocus ? 1 : 0
                    enabled: !Models.Missions.busy
                    text: qsTr("Pick up")
                    Accessible.name: qsTr("Pick up %1").arg(card.modelData.title)
                    onClicked: root.change(card.modelData, "claim", "")
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 34
                visible: card.expanded
                spacing: 12

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
                        Accessible.name: qsTr("Result or pull request link")
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
                        text: card.modelData.issue ? qsTr("Open issue #%1").arg(card.modelData.issue.number) : ""
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

    KScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            x: 14
            width: Math.min(760, scroll.availableWidth - 28)
            spacing: 8

            FocusScope {
                id: composer
                Layout.fillWidth: true
                Layout.topMargin: 2
                implicitHeight: fields.implicitHeight

                Raised { anchors.fill: parent; radius: KodosiTheme.radiusXl; elevation: root.adding ? 2 : 1 }
                Rectangle {
                    anchors.fill: parent
                    radius: KodosiTheme.radiusXl
                    color: "transparent"
                    border.width: 1.5
                    border.color: KodosiTheme.alpha(KodosiTheme.accent, root.adding ? 0.5 : 0)

                    Behavior on border.color { ColorAnimation { duration: KodosiTheme.motionFade } }
                }
                ColumnLayout {
                    id: fields
                    width: parent.width
                    spacing: 0

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 42
                        Layout.leftMargin: 14
                        Layout.rightMargin: 8
                        spacing: 10

                        KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "plus"; strokeWidth: 2.2; color: root.adding ? KodosiTheme.accentStrong : KodosiTheme.inkFaint }
                        TextInput {
                            id: quick
                            Layout.fillWidth: true
                            focus: true
                            text: root.presentation.taskTitle || ""
                            onTextEdited: Models.Missions.setPresentation("taskTitle", text)
                            color: KodosiTheme.ink
                            font.pixelSize: KodosiTheme.fontCallout
                            selectionColor: KodosiTheme.accent
                            selectedTextColor: KodosiTheme.accentInk
                            clip: true
                            objectName: "room.task.title"
                            Accessible.id: objectName
                            Accessible.name: qsTr("Add a task")
                            Accessible.role: Accessible.EditableText
                            onAccepted: root.add()

                            PlainLabel { visible: quick.text.length === 0; text: qsTr("Add a task"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCallout }
                        }
                        KButton {
                            id: filterButton
                            compact: true
                            variant: KButton.Ghost
                            visible: root.repositories.length > 0
                            iconName: "repository"
                            text: (root.repositories.find(repository => repository.id === root.repositoryFilter) || { name: qsTr("All") }).name
                            Accessible.name: qsTr("Show tasks for one repository")
                            ToolTip.visible: hovered
                            ToolTip.text: Accessible.name
                            ToolTip.delay: Tip.delay
                            onClicked: filterMenu.popup(filterButton, 0, filterButton.height + 4)

                            KMenu {
                                id: filterMenu

                                KMenuItem { text: qsTr("All repositories"); onTriggered: Models.Missions.setPresentation("taskFilter", "") }
                                Instantiator {
                                    model: root.repositories
                                    onObjectAdded: (index, item) => filterMenu.insertItem(index + 1, item)
                                    onObjectRemoved: (index, item) => filterMenu.removeItem(item)
                                    delegate: KMenuItem {
                                        required property var modelData
                                        text: modelData.name
                                        iconName: "repository"
                                        onTriggered: Models.Missions.setPresentation("taskFilter", modelData.id)
                                    }
                                }
                            }
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
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: 14
                        Layout.rightMargin: 14
                        Layout.bottomMargin: 12
                        visible: root.adding
                        spacing: 8

                        TextArea {
                            id: description
                            Layout.fillWidth: true
                            leftPadding: 0
                            rightPadding: 0
                            text: root.presentation.taskDescription || ""
                            onTextChanged: {
                                if (text !== (root.presentation.taskDescription || ""))
                                    Models.Missions.setPresentation("taskDescription", text);
                            }
                            wrapMode: TextEdit.Wrap
                            selectByMouse: true
                            placeholderText: qsTr("Context for whoever picks it up")
                            color: KodosiTheme.ink
                            placeholderTextColor: KodosiTheme.inkFaint
                            selectionColor: KodosiTheme.accent
                            selectedTextColor: KodosiTheme.accentInk
                            font.pixelSize: KodosiTheme.fontBody
                            background: null
                            objectName: "room.task.description"
                            Accessible.id: objectName
                            Accessible.name: qsTr("Context for whoever picks it up")
                        }
                        Flow {
                            Layout.fillWidth: true
                            visible: root.repositories.length > 0
                            spacing: 6

                            Repeater {
                                model: root.repositories

                                AbstractButton {
                                    id: choice

                                    required property var modelData
                                    readonly property bool on: root.chosen.indexOf(modelData.id) >= 0

                                    implicitWidth: tag.implicitWidth
                                    implicitHeight: tag.implicitHeight
                                    scale: pressed ? 0.95 : 1
                                    Accessible.name: modelData.name
                                    Accessible.checkable: true
                                    Accessible.checked: on
                                    contentItem: Tag {
                                        id: tag
                                        text: choice.modelData.name
                                        iconName: choice.on ? "check" : "repository"
                                        tone: choice.on ? Tag.Accent : Tag.Neutral
                                    }
                                    onClicked: root.toggle(modelData.id)

                                    KFocusIndicator { active: choice.visualFocus }
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
            Lane {
                key: "open"
                title: qsTr("Up for grabs")
                values: root.tasks.filter(task => !task.closed && !task.assignedTo)
            }
            Lane {
                key: "progress"
                title: qsTr("In progress")
                values: root.tasks.filter(task => !task.closed && !!task.assignedTo)
            }
            Lane {
                key: "done"
                title: qsTr("Done")
                folds: true
                values: root.tasks.filter(task => task.closed)
            }
            Item { Layout.preferredHeight: 20 }
        }
    }
}
