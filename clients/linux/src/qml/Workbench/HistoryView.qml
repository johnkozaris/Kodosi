pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    function openModal() {
        Models.ConversationHistory.open(Models.ConversationHistory.Claude, "");
        open();
    }

    focus: true
    height: Math.min(640, parent ? parent.height - 32 : 640)
    modal: true
    objectName: "panel.history"
    padding: 18
    parent: Overlay.overlay
    width: Math.min(900, parent ? parent.width - 32 : 900)
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0

    onClosed: {
        Models.DesktopFiles.cancelDirectory();
        Models.ConversationHistory.reset();
    }

    Connections {
        function onDirectoryPicked(purpose, path) {
            if (purpose !== "history" || !root.opened)
                return;
            Models.ConversationHistory.open(Models.ConversationHistory.provider, path);
        }

        target: Models.DesktopFiles
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        RowLayout {
            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.textPrimary
                font.pixelSize: 18
                text: qsTr("History")
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Close conversation preview")
                glyph: "close"
                objectName: "panel.history.close"

                onClicked: root.close()
            }
        }
        RowLayout {
            KComboBox {
                Accessible.id: objectName
                Accessible.name: qsTr("Provider")
                currentIndex: Models.ConversationHistory.provider
                model: ["Claude", "Copilot"]
                objectName: "panel.history.provider"

                onActivated: Models.ConversationHistory.open(
                    currentIndex === 0
                        ? Models.ConversationHistory.Claude
                        : Models.ConversationHistory.Copilot,
                    Models.ConversationHistory.directory)
            }
            PlainLabel {
                Layout.fillWidth: true
                color: KodosiTheme.textSecondary
                elide: Text.ElideMiddle
                text: Models.ConversationHistory.directory || qsTr("All projects")
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.history.all-projects"
                text: qsTr("All projects")
                visible: Models.ConversationHistory.directory.length > 0
                onClicked: Models.ConversationHistory.open(
                    Models.ConversationHistory.provider, "")
            }
            KButton {
                Accessible.id: objectName
                objectName: "panel.history.folder"
                text: qsTr("Choose folder…")

                onClicked: Models.DesktopFiles.requestDirectory("history", Models.ConversationHistory.directory)
            }
        }
        RowLayout {
            Layout.fillHeight: true
            Layout.fillWidth: true
            spacing: 14

            ListView {
                id: conversations

                Layout.fillHeight: true
                Layout.preferredWidth: Math.min(260, root.width * 0.36)
                clip: true
                model: Models.ConversationHistory.conversations

                ScrollBar.vertical: KScrollBar {
                }
                delegate: KItemDelegate {
                    id: conversationRow

                    required property var modelData

                    Accessible.id: objectName
                    objectName: "panel.history.conversation." + conversationRow.modelData.nativeConversationId
                    text: conversationRow.modelData.title || conversationRow.modelData.nativeConversationId
                    width: conversations.width

                    onClicked: Models.ConversationHistory.preview(conversationRow.modelData.nativeConversationId)
                }
                footer: KButton {
                    Accessible.id: objectName
                    enabled: !Models.ConversationHistory.busy
                    objectName: "panel.history.more-conversations"
                    text: qsTr("More conversations")
                    visible: Models.ConversationHistory.hasMore

                    onClicked: Models.ConversationHistory.loadMore()
                }
            }
            KScrollView {
                Layout.fillHeight: true
                Layout.fillWidth: true

                ColumnLayout {
                    spacing: 12
                    width: parent.width

                    RowLayout {
                        KButton {
                            Accessible.id: objectName
                            enabled: !Models.ConversationHistory.busy && Models.ConversationHistory.hasOlder
                            objectName: "panel.history.earlier-messages"
                            text: qsTr("Earlier")
                            onClicked: Models.ConversationHistory.loadOlder()
                        }
                        KButton {
                            Accessible.id: objectName
                            enabled: !Models.ConversationHistory.busy && Models.ConversationHistory.hasNewer
                            objectName: "panel.history.later-messages"
                            text: qsTr("Later")
                            onClicked: Models.ConversationHistory.loadNewer()
                        }
                        KButton {
                            Accessible.id: objectName
                            enabled: !Models.ConversationHistory.busy && Models.ConversationHistory.hasNewer
                            objectName: "panel.history.latest-messages"
                            text: qsTr("Latest")
                            onClicked: Models.ConversationHistory.loadLatest()
                        }
                    }
                    Repeater {
                        model: Models.ConversationHistory.entries

                        delegate: ColumnLayout {
                            id: entry1

                            required property var modelData
                            required property int index
                            property bool expanded: false
                            readonly property string content: modelData.content || ""

                            Layout.fillWidth: true
                            spacing: 4

                            PlainLabel {
                                color: KodosiTheme.textSecondary
                                font.pixelSize: 10
                                text: entry1.modelData.role || ""
                            }
                            KReadOnlyText {
                                Layout.fillWidth: true
                                text: entry1.expanded || entry1.content.length <= 2000 ? entry1.content : entry1.content.slice(0, 2000) + "…"
                                visible: entry1.modelData.role !== "tool" || entry1.expanded
                                wrapMode: TextEdit.Wrap
                            }
                            KButton {
                                Accessible.id: objectName
                                objectName: "panel.history.message." + entry1.index + ".expand"
                                text: entry1.expanded ? qsTr("Show less") : entry1.modelData.role === "tool" ? qsTr("Show tool output") : qsTr("Show more")
                                visible: entry1.content.length > 2000 || entry1.modelData.role === "tool"
                                variant: KButton.Quiet
                                onClicked: entry1.expanded = !entry1.expanded
                            }
                        }
                    }
                    PlainLabel {
                        color: KodosiTheme.textSecondary
                        text: qsTr("Select a saved conversation to preview it.")
                        visible: Models.ConversationHistory.entries.length === 0 && !Models.ConversationHistory.busy
                    }
                }
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.danger
            text: Models.ConversationHistory.error
            visible: Models.ConversationHistory.error.length > 0
            wrapMode: Text.WordWrap
        }
        RowLayout {
            KBusyIndicator {
                running: Models.ConversationHistory.busy
                visible: running
            }
            Item {
                Layout.fillWidth: true
            }
            KButton {
                Accessible.id: objectName
                enabled: !Models.ConversationHistory.busy && !!Models.ConversationHistory.selectedConversation.nativeConversationId
                objectName: "panel.history.start"
                text: qsTr("Resume in new terminal")

                onClicked: {
                    const selected = Models.ConversationHistory.selectedConversation;
                    if (Models.SessionActions.resume(Models.ConversationHistory.providerId,
                            selected.nativeConversationId, selected.workingDirectory)) {
                        root.close();
                    }
                }
            }
        }
    }
}
