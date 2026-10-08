pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

Item {
    id: root
    readonly property var presentation: Models.Missions.presentation
    readonly property int canvas: presentation.canvas || 0
    readonly property bool conversation: presentation.conversation !== false
    readonly property bool narrow: width < 720
    signal inspectSessionRequested(string sessionId)
    SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal
        handle: Rectangle { implicitWidth: 1; color: KodosiTheme.seam }
        ColumnLayout {
            SplitView.fillWidth: true
            SplitView.minimumWidth: root.narrow ? 0 : 360
            visible: !root.narrow || !root.conversation
            spacing: 0
            Rectangle {
                Layout.fillWidth: true; Layout.preferredHeight: 52; color: KodosiTheme.surface
                RowLayout {
                    anchors.fill: parent; anchors.margins: 8
                    Item {
                        Layout.fillWidth: true; Layout.fillHeight: true
                        Rectangle {
                            x: tabs.itemAt(root.canvas) ? tabs.itemAt(root.canvas).x : 0
                            width: tabs.itemAt(root.canvas) ? tabs.itemAt(root.canvas).width : 80
                            height: 36; radius: 9; color: KodosiTheme.surfaceSelected
                            Behavior on x { NumberAnimation { duration: KodosiTheme.motionFast; easing.type: Easing.OutCubic } }
                            Behavior on width { NumberAnimation { duration: KodosiTheme.motionFast; easing.type: Easing.OutCubic } }
                        }
                        Row {
                            Repeater {
                                id: tabs
                                model: [ {label: qsTr("Terminals"), icon: "terminal", count: Models.Missions.sessionIds.length}, {label: qsTr("Tasks"), icon: "tasks", count: (Models.Missions.room.tasks || []).filter(task => !task.closed).length}, {label: qsTr("Repositories"), icon: "repository", count: (Models.Missions.room.repositories || []).length} ]
                                delegate: ItemDelegate {
                                    id: canvasTab
                                    required property var modelData
                                    required property int index
                                    height: 36
                                    implicitWidth: tabContent.implicitWidth + 20
                                    Accessible.name: modelData.label
                                    Accessible.role: Accessible.PageTab
                                    Accessible.selected: root.canvas === index
                                    objectName: "room.canvas." + index
                                    background: null
                                    contentItem: RowLayout {
                                        id: tabContent; spacing: 6
                                        KIcon { name: canvasTab.modelData.icon; color: root.canvas === canvasTab.index ? KodosiTheme.textPrimary : KodosiTheme.textSecondary }
                                        PlainLabel { text: canvasTab.modelData.label; font.pixelSize: 12; color: root.canvas === canvasTab.index ? KodosiTheme.textPrimary : KodosiTheme.textSecondary }
                                        PlainLabel { visible: canvasTab.modelData.count > 0; text: canvasTab.modelData.count; font.pixelSize: 10; color: KodosiTheme.textSecondary }
                                    }
                                    onClicked: Models.Missions.setPresentation("canvas", index)
                                }
                            }
                        }
                    }
                    KIconButton { glyph: "plus"; visible: root.canvas === 0; Accessible.name: qsTr("New terminal"); onClicked: Models.SessionActions.createInRoom(Models.Missions.selectedMissionId, Models.DesktopSettings.effectiveWorkingDirectory) }
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: KodosiTheme.seam }
            StackLayout {
                Layout.fillWidth: true; Layout.fillHeight: true
                currentIndex: root.canvas
                ColumnLayout {
                    spacing: 0
                    Flickable {
                        Layout.fillWidth: true; Layout.preferredHeight: !terminal.active && Models.Missions.sessionIds.length ? 42 : 0
                        visible: !terminal.active
                        contentWidth: terminalTabs.implicitWidth; clip: true
                        Row {
                            id: terminalTabs; leftPadding: 8; rightPadding: 8; spacing: 6
                            Repeater {
                                model: Models.Missions.sessionIds
                                delegate: KButton {
                                    required property string modelData
                                    readonly property var session: Models.Sessions.presentationForSession(modelData)
                                    text: session.name || qsTr("Terminal"); iconName: "terminal"
                                    variant: root.presentation.terminal === modelData ? KButton.Secondary : KButton.Quiet
                                    onClicked: Models.SessionActions.activateInRoom(modelData)
                                    objectName: "room.terminal." + modelData
                                }
                            }
                        }
                    }
                    Item {
                        Layout.fillWidth: true; Layout.fillHeight: true
                        Loader {
                            id: terminal
                            anchors.fill: parent
                            active: root.visible && root.canvas === 0 && !!root.presentation.terminal && Models.DesktopState.stagedSessionIds.indexOf(root.presentation.terminal) >= 0 && Models.Missions.sessionIds.indexOf(root.presentation.terminal) >= 0
                            sourceComponent: TerminalTile {
                                sessionId: root.presentation.terminal || ""
                                roomEmbedded: true
                                interactionEnabled: root.visible && !Models.DesktopState.modalOpen
                                onInspectSessionRequested: (id, name) => root.inspectSessionRequested(id)
                                onShareSessionRequested: (id, name) => root.inspectSessionRequested(id)
                                Component.onCompleted: Qt.callLater(forceTerminalFocus)
                            }
                        }
                        ColumnLayout {
                            anchors.centerIn: parent; visible: !terminal.active; spacing: 20
                            KIcon { name: "terminal"; implicitWidth: 52; implicitHeight: 52; color: KodosiTheme.accent; Layout.alignment: Qt.AlignHCenter }
                            KButton { text: qsTr("New terminal"); iconName: "plus"; variant: KButton.Primary; onClicked: Models.SessionActions.createInRoom(Models.Missions.selectedMissionId, Models.DesktopSettings.effectiveWorkingDirectory) }
                        }
                    }
                }
                Item {
                    RoomSkeleton { anchors.fill: parent; visible: !Models.Missions.room.roomId && !root.presentation.failure }
                    KButton { anchors.centerIn: parent; visible: !Models.Missions.room.roomId && !!root.presentation.failure; text: qsTr("Try again"); iconName: "refresh"; onClicked: Models.Missions.roomAction({type: "read"}) }
                    RoomTasks { anchors.fill: parent; visible: !!Models.Missions.room.roomId }
                }
                Item {
                    RoomSkeleton { anchors.fill: parent; visible: !Models.Missions.room.roomId && !root.presentation.failure }
                    KButton { anchors.centerIn: parent; visible: !Models.Missions.room.roomId && !!root.presentation.failure; text: qsTr("Try again"); iconName: "refresh"; onClicked: Models.Missions.roomAction({type: "read"}) }
                    RoomRepositories { anchors.fill: parent; visible: !!Models.Missions.room.roomId }
                }
            }
        }
        Rectangle {
            SplitView.preferredWidth: root.narrow ? root.width : 370
            SplitView.minimumWidth: 300
            SplitView.maximumWidth: root.narrow ? root.width : 500
            SplitView.fillWidth: root.narrow
            color: KodosiTheme.canvas
            visible: root.conversation
            RoomConversation { anchors.fill: parent }
        }
    }
}
