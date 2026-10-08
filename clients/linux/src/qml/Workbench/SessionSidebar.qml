pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root
    signal detailsRequested(string sessionId)
    signal newSessionRequested(string folder)
    signal historyRequested
    Accessible.id: objectName
    Accessible.name: qsTr("Terminals")
    Accessible.role: Accessible.Pane
    objectName: "sidebar.sessions"

    Rectangle { anchors.fill: parent; color: KodosiTheme.surface }
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 8
        RowLayout {
            KButton {
                Accessible.id: objectName
                Layout.fillWidth: true
                objectName: "sidebar.sessions.new"
                text: qsTr("New terminal")
                variant: KButton.Primary
                onClicked: root.newSessionRequested("")
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("New terminal in folder")
                objectName: "sidebar.sessions.folder"
                glyph: "folder"
                onClicked: Models.DesktopFiles.requestDirectory("new", Models.DesktopSettings.effectiveWorkingDirectory)
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Collapse sidebar")
                objectName: "sidebar.sessions.collapse"
                glyph: "sidebar"
                onClicked: Models.DesktopState.sidebarOpen = false
            }
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            ColumnLayout {
                width: parent.width
                spacing: 4
                Repeater {
                    model: Models.Sessions.folderGroups
                    delegate: ColumnLayout {
                        id: folder
                        required property var modelData
                        property bool expanded: true
                        Layout.fillWidth: true
                        spacing: 2
                        RowLayout {
                            KButton {
                                Accessible.id: objectName
                                Layout.fillWidth: true
                                objectName: "sidebar.folder." + folder.modelData.key
                                text: folder.modelData.host ? folder.modelData.name + " · " + folder.modelData.host : folder.modelData.name
                                variant: KButton.Quiet
                                onClicked: folder.expanded = !folder.expanded
                            }
                            KIconButton {
                                Accessible.id: objectName
                                Accessible.name: qsTr("New terminal in %1").arg(folder.modelData.name)
                                objectName: "sidebar.folder.new." + folder.modelData.key
                                glyph: "plus"
                                visible: !!folder.modelData.directory
                                onClicked: root.newSessionRequested(folder.modelData.directory)
                            }
                        }
                        Repeater {
                            model: folder.expanded ? folder.modelData.sessions : []
                            delegate: ItemDelegate {
                                id: row
                                required property var modelData
                                Layout.fillWidth: true
                                implicitHeight: 34
                                Accessible.id: objectName
                                Accessible.name: modelData.name
                                Accessible.selected: Models.DesktopState.selectedSessionId === modelData.id
                                objectName: "sidebar.session." + modelData.id
                                background: Rectangle { color: Models.DesktopState.selectedSessionId === row.modelData.id ? KodosiTheme.surfaceRaised : KodosiTheme.surface }
                                contentItem: RowLayout {
                                    PlainLabel { Layout.fillWidth: true; text: row.modelData.name; color: KodosiTheme.textPrimary; elide: Text.ElideRight }
                                    KIconButton {
                                        Accessible.id: objectName
                                        Accessible.name: qsTr("Terminal details")
                                        objectName: row.objectName + ".details"
                                        glyph: "document"
                                        onClicked: root.detailsRequested(row.modelData.id)
                                    }
                                    KIconButton {
                                        Accessible.id: objectName
                                        Accessible.name: qsTr("Minimize")
                                        objectName: row.objectName + ".minimize"
                                        glyph: "minus"
                                        visible: Models.DesktopState.stagedSessionIds.indexOf(row.modelData.id) >= 0
                                        onClicked: Models.SessionActions.minimize(row.modelData.id)
                                    }
                                }
                                onClicked: Models.SessionActions.activate(modelData.id)
                            }
                        }
                    }
                }
            }
        }
        KButton {
            Accessible.id: objectName
            Layout.fillWidth: true
            objectName: "sidebar.sessions.history"
            text: qsTr("History")
            variant: KButton.Quiet
            onClicked: root.historyRequested()
        }
    }
}
