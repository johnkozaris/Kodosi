pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    readonly property var history: Models.ConversationHistory
    readonly property bool hasList: history.busy || history.conversations.length > 0
    readonly property string program: history.provider === Models.ConversationHistory.Claude ? "claude" : "copilot"
    readonly property string selectedId: history.selectedConversation.nativeConversationId || ""

    function openModal() {
        Models.ConversationHistory.open(Models.ConversationHistory.Claude, "");
        open();
    }
    function folderName(path) {
        const parts = (path || "").split("/").filter(part => part.length > 0);
        return parts.length > 0 ? parts[parts.length - 1] : "";
    }

    focus: true
    height: Math.min(640, parent ? parent.height - 32 : 640)
    modal: true
    objectName: "panel.history"
    padding: 0
    parent: Overlay.overlay
    radius: KodosiTheme.radiusSheet
    elevation: 3
    width: Math.min(940, parent ? parent.width - 32 : 940)
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
    contentItem: ColumnLayout {
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 60
            Layout.leftMargin: 22
            Layout.rightMargin: 14
            spacing: 10

            PlainLabel {
                Layout.fillWidth: true
                font.pixelSize: KodosiTheme.fontTitle
                font.weight: Font.DemiBold
                text: qsTr("Resume a conversation")
            }
            SegmentedPill {
                compact: true
                identifier: "panel.history.provider"
                options: [
                    { value: Models.ConversationHistory.Claude, label: "Claude" },
                    { value: Models.ConversationHistory.Copilot, label: "Copilot" }
                ]
                currentValue: root.history.provider
                onActivated: value => Models.ConversationHistory.open(value, root.history.directory)
            }
            KButton {
                Accessible.id: objectName
                compact: true
                iconName: "folder"
                objectName: "panel.history.folder"
                text: root.history.directory.length > 0 ? root.folderName(root.history.directory) : qsTr("All projects")

                onClicked: Models.DesktopFiles.requestDirectory("history", root.history.directory)
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("All projects")
                glyph: "close"
                size: 24
                objectName: "panel.history.all-projects"
                visible: root.history.directory.length > 0
                onClicked: Models.ConversationHistory.open(root.history.provider, "")
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Close")
                glyph: "close"
                objectName: "panel.history.close"

                onClicked: root.close()
            }
        }
        RowLayout {
            Layout.fillHeight: true
            Layout.fillWidth: true
            spacing: 0

            Well {
                Layout.fillHeight: true
                Layout.preferredWidth: 290
                Layout.leftMargin: 14
                visible: root.hasList
                radius: KodosiTheme.radiusXl

                ListView {
                    id: conversations

                    anchors.fill: parent
                    anchors.margins: 8
                    clip: true
                    spacing: 2
                    model: root.history.conversations

                    ScrollBar.vertical: KScrollBar {}
                    delegate: AbstractButton {
                        id: conversation

                        required property var modelData
                        readonly property bool selected: root.selectedId === modelData.nativeConversationId

                        Accessible.id: objectName
                        Accessible.name: title.text
                        Accessible.selected: selected
                        objectName: "panel.history.conversation." + modelData.nativeConversationId
                        width: conversations.width
                        height: Math.max(48, labels.implicitHeight + 20)
                        hoverEnabled: true

                        contentItem: Item {
                            ColumnLayout {
                                id: labels
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.leftMargin: 12
                                anchors.rightMargin: 12
                                spacing: 4

                                PlainLabel {
                                    id: title
                                    Layout.fillWidth: true
                                    text: conversation.modelData.title || root.folderName(conversation.modelData.workingDirectory) || conversation.modelData.nativeConversationId
                                    font.weight: Font.Medium
                                    wrapMode: Text.WordWrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                }
                                RowLayout {
                                    spacing: 5
                                    KIcon { Layout.preferredWidth: 10; Layout.preferredHeight: 10; name: "folder"; color: KodosiTheme.inkFaint }
                                    PlainLabel {
                                        Layout.fillWidth: true
                                        text: root.folderName(conversation.modelData.workingDirectory)
                                        color: KodosiTheme.inkFaint
                                        font.pixelSize: KodosiTheme.fontCaption
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                        }
                        background: Item {
                            Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; visible: conversation.selected }
                            Rectangle {
                                anchors.fill: parent
                                radius: KodosiTheme.radiusLg
                                color: KodosiTheme.alpha(KodosiTheme.ink, !conversation.selected && conversation.hovered ? 0.06 : 0)
                            }
                        }

                        onClicked: Models.ConversationHistory.preview(modelData.nativeConversationId)
                    }
                    footer: Item {
                        width: conversations.width
                        height: more.visible ? 40 : 0

                        KButton {
                            id: more
                            anchors.centerIn: parent
                            Accessible.id: objectName
                            compact: true
                            variant: KButton.Ghost
                            enabled: !root.history.busy
                            objectName: "panel.history.more-conversations"
                            text: qsTr("Show more")
                            visible: root.history.hasMore

                            onClicked: Models.ConversationHistory.loadMore()
                        }
                    }
                }
            }
            Item {
                Layout.fillHeight: true
                Layout.fillWidth: true

                EmptyState {
                    anchors.centerIn: parent
                    width: Math.min(360, parent.width - 40)
                    visible: root.selectedId.length === 0
                    title: root.hasList ? qsTr("Pick a conversation") : qsTr("Nothing saved here")
                    message: root.hasList ? qsTr("You see it here before you resume it.") : qsTr("Choose another project, or switch the agent.")
                    art: AgentMark { program: root.program; size: 52; asleep: true }
                }
                KScrollView {
                    id: preview
                    anchors.fill: parent
                    visible: root.selectedId.length > 0
                    contentWidth: availableWidth

                    ColumnLayout {
                        x: 20
                        width: preview.availableWidth - 40
                        spacing: 16

                        Item { Layout.preferredHeight: 4 }
                        Repeater {
                            model: root.history.entries

                            RowLayout {
                                id: entry

                                required property var modelData
                                required property int index
                                property bool expanded: false
                                readonly property string content: modelData.content || ""
                                readonly property bool own: modelData.role === "user"
                                readonly property bool tool: modelData.role === "tool"

                                Layout.fillWidth: true
                                spacing: 10

                                AgentMark { Layout.alignment: Qt.AlignTop; visible: !entry.own; program: root.program; size: 22; asleep: entry.tool }
                                Item { visible: entry.own; Layout.fillWidth: true; Layout.minimumWidth: 60 }
                                Rectangle {
                                    Layout.fillWidth: !entry.own
                                    Layout.maximumWidth: entry.own ? preview.availableWidth * 0.72 : -1
                                    implicitWidth: bubble.implicitWidth + (entry.own ? 24 : 0)
                                    implicitHeight: bubble.implicitHeight + (entry.own ? 16 : 0)
                                    radius: 16
                                    color: entry.own ? KodosiTheme.accentSoft : "transparent"

                                    ColumnLayout {
                                        id: bubble
                                        anchors.fill: parent
                                        anchors.margins: entry.own ? 12 : 0
                                        anchors.topMargin: entry.own ? 8 : 0
                                        anchors.bottomMargin: entry.own ? 8 : 0
                                        spacing: 6

                                        KReadOnlyText {
                                            Layout.fillWidth: true
                                            text: entry.expanded || entry.content.length <= 2000 ? entry.content : entry.content.slice(0, 2000) + "…"
                                            visible: !entry.tool || entry.expanded
                                            font.family: entry.tool ? "monospace" : Application.font.family
                                            color: entry.tool ? KodosiTheme.inkMuted : KodosiTheme.ink
                                        }
                                        KButton {
                                            Accessible.id: objectName
                                            compact: true
                                            variant: KButton.Ghost
                                            objectName: "panel.history.message." + entry.index + ".expand"
                                            text: entry.expanded ? qsTr("Show less") : entry.tool ? qsTr("Show tool output") : qsTr("Show more")
                                            visible: entry.content.length > 2000 || entry.tool
                                            onClicked: entry.expanded = !entry.expanded
                                        }
                                    }
                                }
                            }
                        }
                        ShimmerText {
                            visible: root.history.busy
                            text: qsTr("Reading")
                            active: visible
                            color: KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                        }
                        Item { Layout.preferredHeight: 4 }
                    }
                }
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            Layout.leftMargin: 22
            Layout.rightMargin: 22
            Layout.topMargin: 8
            color: KodosiTheme.danger
            font.pixelSize: KodosiTheme.fontFootnote
            text: root.history.error
            visible: root.history.error.length > 0
            wrapMode: Text.WordWrap
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 64
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            spacing: 4

            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Earlier")
                glyph: "chevron-up"
                size: 26
                visible: root.selectedId.length > 0
                enabled: !root.history.busy && root.history.hasOlder
                objectName: "panel.history.earlier-messages"
                onClicked: Models.ConversationHistory.loadOlder()
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Later")
                glyph: "chevron-down"
                size: 26
                visible: root.selectedId.length > 0
                enabled: !root.history.busy && root.history.hasNewer
                objectName: "panel.history.later-messages"
                onClicked: Models.ConversationHistory.loadNewer()
            }
            KButton {
                Accessible.id: objectName
                compact: true
                variant: KButton.Ghost
                visible: root.selectedId.length > 0 && root.history.hasNewer
                enabled: !root.history.busy
                objectName: "panel.history.latest-messages"
                text: qsTr("Latest")
                onClicked: Models.ConversationHistory.loadLatest()
            }
            Item { Layout.fillWidth: true }
            KButton {
                Accessible.id: objectName
                variant: KButton.Primary
                large: true
                iconName: "play"
                enabled: !root.history.busy && root.selectedId.length > 0
                objectName: "panel.history.start"
                text: qsTr("Resume in a new terminal")

                onClicked: {
                    const selected = root.history.selectedConversation;
                    if (Models.SessionActions.resume(root.history.providerId,
                            selected.nativeConversationId, selected.workingDirectory)) {
                        root.close();
                    }
                }
            }
        }
    }
}
