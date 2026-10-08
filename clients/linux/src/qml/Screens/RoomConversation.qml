pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi 1.0
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root
    readonly property var presentation: Models.Missions.presentation
    readonly property var room: Models.Missions.room
    readonly property var messages: room.messages || []
    property bool copied: false
    spacing: 0
    objectName: "room.conversation"
    onVisibleChanged: {
        if (visible && root.presentation.followLatest !== false) {
            Qt.callLater(function() {
                transcript.forceLayout();
                transcript.positionViewAtEnd();
            });
        }
    }
    function rememberPosition() {
        Models.Missions.setPresentation("followLatest", transcript.atYEnd);
        let index = transcript.indexAt(1, transcript.contentY + transcript.spacing + 1);
        if (index < 0) index = 0;
        const item = transcript.itemAtIndex(index);
        if (item && root.messages[index]) {
            Models.Missions.setPresentation("chatAnchor", root.messages[index].id);
            Models.Missions.setPresentation("chatOffset", transcript.contentY - item.y);
        }
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 52
        color: KodosiTheme.canvas
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 12
            KIcon { name: "chat" }
            PlainLabel { text: qsTr("Conversation"); font.weight: Font.DemiBold; Layout.fillWidth: true }
            KIconButton {
                glyph: root.copied ? "check" : "agent"
                Accessible.name: qsTr("Copy agent instructions")
                objectName: "room.agent.instructions"
                onClicked: { Models.Missions.copyAgentInstructions(); root.copied = true; copiedTimer.restart(); }
            }
        }
    }
    Timer { id: copiedTimer; interval: 2000; onTriggered: root.copied = false }
    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: KodosiTheme.seam }
    Item {
        Layout.fillWidth: true
        Layout.fillHeight: true
        RoomSkeleton { anchors.fill: parent; visible: !root.room.roomId && !root.presentation.failure }
        ColumnLayout {
            anchors.centerIn: parent
            visible: !root.room.roomId && !!root.presentation.failure
            spacing: 14
            KIcon { name: "warning"; implicitWidth: 38; implicitHeight: 38; Layout.alignment: Qt.AlignHCenter }
            KButton { text: qsTr("Try again"); onClicked: Models.Missions.roomAction({ type: "read" }) }
        }
        KIcon { anchors.centerIn: parent; name: "chat"; implicitWidth: 44; implicitHeight: 44; opacity: 0.4; visible: !!root.room.roomId && root.messages.length === 0 }
        ListView {
            id: transcript
            anchors.fill: parent
            anchors.margins: 16
            clip: true
            model: root.messages
            spacing: 20
            rightMargin: 12
            reuseItems: true
            ScrollBar.vertical: KScrollBar {}
            onModelChanged: {
                const roomId = Models.Missions.selectedMissionId;
                const following = root.presentation.followLatest !== false;
                const anchor = root.presentation.chatAnchor;
                const offset = root.presentation.chatOffset || 0;
                Qt.callLater(function() {
                    if (roomId !== Models.Missions.selectedMissionId) return;
                    transcript.forceLayout();
                    if (following) {
                        transcript.positionViewAtEnd();
                    } else {
                        const index = root.messages.findIndex(message => message.id === anchor);
                        if (index < 0) return;
                        transcript.positionViewAtIndex(index, ListView.Beginning);
                        Qt.callLater(function() {
                            if (roomId !== Models.Missions.selectedMissionId || root.messages[index]?.id !== anchor) return;
                            const item = transcript.itemAtIndex(index);
                            if (item) transcript.contentY = item.y + offset;
                        });
                    }
                });
            }
            onMovementEnded: root.rememberPosition()
            header: Item {
                width: transcript.width - transcript.rightMargin
                height: root.room.hasOlder ? 40 : 0
                KIconButton {
                    anchors.centerIn: parent
                    visible: !!root.room.hasOlder
                    glyph: "chevron-up"
                    Accessible.name: qsTr("Earlier messages")
                    objectName: "room.messages.earlier"
                    onClicked: {
                        root.rememberPosition();
                        Models.Missions.setPresentation("followLatest", false);
                        Models.Missions.roomAction({ type: "read", before: root.messages[0].sequence });
                    }
                }
            }
            delegate: RowLayout {
                id: entry
                required property var modelData
                width: transcript.width - transcript.rightMargin
                spacing: 10
                RoomAvatar { name: entry.modelData.authorName; Layout.alignment: Qt.AlignTop; implicitWidth: 26; implicitHeight: 26 }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    RowLayout {
                        Layout.fillWidth: true
                        PlainLabel { text: entry.modelData.authorName; font.pixelSize: 12; font.weight: Font.DemiBold }
                        KIcon { name: "agent"; visible: !!entry.modelData.agent; implicitWidth: 12; implicitHeight: 12; color: KodosiTheme.accent }
                        PlainLabel { text: entry.modelData.agent || ""; visible: !!entry.modelData.agent; font.pixelSize: 10; color: KodosiTheme.accent; elide: Text.ElideRight; Layout.fillWidth: true }
                        Item { Layout.fillWidth: true }
                        PlainLabel { text: Qt.formatTime(new Date(entry.modelData.createdAt), "hh:mm"); font.pixelSize: 10; color: KodosiTheme.textTertiary }
                    }
                    TextEdit {
                        Layout.fillWidth: true
                        text: entry.modelData.text
                        color: KodosiTheme.textPrimary
                        font.pixelSize: 13
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        textFormat: TextEdit.PlainText
                        Accessible.name: entry.modelData.text
                    }
                }
            }
            add: Transition { NumberAnimation { properties: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFast } }
        }
        KButton {
            anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottomMargin: 8
            visible: !transcript.atYEnd && root.messages.length > 0
            text: qsTr("Latest")
            iconName: "chevron-down"
            onClicked: { transcript.positionViewAtEnd(); Models.Missions.setPresentation("followLatest", true); }
        }
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.margins: 12
        Layout.preferredHeight: Math.min(146, Math.max(46, editor.implicitHeight + 16))
        radius: 16
        color: KodosiTheme.input
        border.color: editor.activeFocus ? KodosiTheme.accent : KodosiTheme.seam
        border.width: 1
        RowLayout {
            anchors.fill: parent; anchors.margins: 7; spacing: 8
            ScrollView {
                Layout.fillWidth: true; Layout.fillHeight: true
                TextArea {
                    id: editor
                    text: root.presentation.message || ""
                    onTextChanged: {
                        if (text !== (root.presentation.message || ""))
                            Models.Missions.setPresentation("message", text);
                    }
                    placeholderText: qsTr("Message")
                    color: KodosiTheme.textPrimary
                    placeholderTextColor: KodosiTheme.placeholderText
                    wrapMode: TextEdit.Wrap
                    selectByMouse: true
                    font.pixelSize: 13
                    background: null
                    objectName: "room.message.input"
                    Accessible.name: qsTr("Message the room")
                    Keys.onPressed: event => {
                        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                            if (!Models.Missions.busy && text.trim().length) root.send();
                            event.accepted = true;
                        }
                    }
                }
            }
            KIconButton {
                glyph: "send"
                Accessible.name: qsTr("Send message")
                objectName: "room.message.send"
                Layout.alignment: Qt.AlignBottom
                enabled: !Models.Missions.busy && editor.text.trim().length > 0
                onClicked: root.send()
            }
            KBusyIndicator { visible: Models.Missions.busy; running: visible; Layout.preferredWidth: 20; Layout.preferredHeight: 20 }
        }
    }
    function send() { Models.Missions.roomAction({ type: "post", text: editor.text.trim() }); }
}
