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
    readonly property var messages: room.messages || []
    readonly property var targets: Mentions.targets(Models.Missions.members, Models.Missions.sessionIds)
    readonly property string query: editor.activeFocus && dismissed !== Mentions.query(editor.text) ? Mentions.query(editor.text) : ""
    readonly property var suggestions: Mentions.suggestions(query, targets, Models.Account.userId)
    readonly property bool canSend: !Models.Missions.busy && editor.text.trim().length > 0
    property bool copied: false
    property string dismissed: ""
    property int highlighted: 0

    signal terminalRequested(string sessionId)

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
    function send() {
        dismissed = "";
        Models.Missions.roomAction({ type: "post", text: editor.text.trim() });
    }
    function pick(target) {
        Models.Missions.setPresentation("message", Mentions.complete(editor.text, target));
        editor.cursorPosition = editor.text.length;
        highlighted = 0;
    }
    function dayLabel(date) {
        const today = new Date();
        const start = value => new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
        const days = Math.round((start(today) - start(date)) / 86400000);
        return days === 0 ? qsTr("Today") : days === 1 ? qsTr("Yesterday") : date.toLocaleDateString(Qt.locale(), "d MMMM");
    }

    objectName: "room.conversation"
    Accessible.id: objectName
    onQueryChanged: highlighted = 0
    onVisibleChanged: {
        if (visible && root.presentation.followLatest !== false) {
            Qt.callLater(function() {
                transcript.forceLayout();
                transcript.positionViewAtEnd();
            });
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 46
            Layout.leftMargin: 16
            Layout.rightMargin: 10

            PlainLabel { text: qsTr("Conversation"); font.weight: Font.DemiBold; Layout.fillWidth: true }
            KButton {
                compact: true
                variant: KButton.Ghost
                iconName: root.copied ? "check" : "agent"
                text: root.copied ? qsTr("Copied") : qsTr("Bring an agent")
                Accessible.name: qsTr("Copy agent instructions")
                objectName: "room.agent.instructions"
                Accessible.id: objectName
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Copy the instructions. Paste them into your agent.")
                ToolTip.delay: 500
                onClicked: { Models.Missions.copyAgentInstructions(); root.copied = true; copiedTimer.restart(); }
            }
        }
        Timer { id: copiedTimer; interval: 2000; onTriggered: root.copied = false }
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            RoomSkeleton { anchors.fill: parent; visible: !root.room.roomId && !root.presentation.failure }
            EmptyState {
                anchors.centerIn: parent
                width: parent.width - 48
                visible: !root.room.roomId && !!root.presentation.failure
                title: qsTr("The conversation did not load")
                KButton { text: qsTr("Try again"); variant: KButton.Primary; onClicked: Models.Missions.roomAction({ type: "read" }) }
            }
            ColumnLayout {
                anchors.centerIn: parent
                width: parent.width - 44
                visible: !!root.room.roomId && root.messages.length === 0
                spacing: 16

                PlainLabel {
                    Layout.fillWidth: true
                    text: qsTr("This is %1").arg(Models.Missions.selectedMission.name || "")
                    font.pixelSize: KodosiTheme.fontHeadline
                    font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                }
                Repeater {
                    model: [
                        { icon: "terminal", tint: "#d97757", text: qsTr("Add a terminal. Everyone here can use it.") },
                        { icon: "agent", tint: "#8660d9", text: qsTr("Bring an agent. It reads and writes here.") },
                        { icon: "at", tint: "#4f9bb0", text: qsTr("Type @ to mention a person or a terminal.") }
                    ]
                    delegate: RowLayout {
                        id: step
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 11
                        IconTile { iconName: step.modelData.icon; tint: step.modelData.tint; size: 26 }
                        PlainLabel { Layout.fillWidth: true; text: step.modelData.text; color: KodosiTheme.inkMuted; wrapMode: Text.WordWrap }
                    }
                }
            }
            ListView {
                id: transcript
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 4
                clip: true
                model: root.messages
                spacing: 4
                rightMargin: 10
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
                    height: root.room.hasOlder ? 40 : 8
                    KButton {
                        anchors.centerIn: parent
                        visible: !!root.room.hasOlder
                        compact: true
                        variant: KButton.Ghost
                        iconName: "chevron-up"
                        text: qsTr("Earlier")
                        Accessible.name: qsTr("Earlier messages")
                        objectName: "room.messages.earlier"
                        onClicked: {
                            root.rememberPosition();
                            Models.Missions.setPresentation("followLatest", false);
                            Models.Missions.roomAction({ type: "read", before: root.messages[0].sequence });
                        }
                    }
                }
                footer: Item { width: 1; height: 8 }
                delegate: Column {
                    id: entry

                    required property var modelData
                    required property int index
                    readonly property var previous: index > 0 ? root.messages[index - 1] : null
                    readonly property date sent: new Date(modelData.createdAt)
                    readonly property bool own: modelData.authorId === Models.Account.userId && !modelData.agent
                    readonly property bool isAgent: !!modelData.agent
                    readonly property bool newDay: !previous || new Date(previous.createdAt).toDateString() !== sent.toDateString()
                    readonly property bool grouped: !newDay && !!previous && previous.authorId === modelData.authorId && (previous.agent || "") === (modelData.agent || "")
                        && sent.getTime() - new Date(previous.createdAt).getTime() < 300000
                    readonly property bool mentionsMe: !own && Mentions.mentionsUser(modelData.text, root.targets, Models.Account.userId)
                    readonly property var terminal: modelData.terminalId ? (Models.Sessions.sessions.find(session => session.id === modelData.terminalId) || null) : null

                    width: transcript.width - transcript.rightMargin
                    topPadding: grouped ? 0 : 8
                    spacing: 6

                    Item {
                        width: parent.width
                        height: 26
                        visible: entry.newDay

                        Rectangle {
                            anchors.centerIn: parent
                            width: day.implicitWidth + 20
                            height: 20
                            radius: 10
                            color: KodosiTheme.alpha(KodosiTheme.ink, 0.07)
                            PlainLabel { id: day; anchors.centerIn: parent; text: root.dayLabel(entry.sent); color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
                        }
                    }
                    Item {
                        width: parent.width
                        height: row.implicitHeight + (entry.mentionsMe ? 12 : 0)

                        Rectangle {
                            anchors.fill: parent
                            visible: entry.mentionsMe
                            radius: KodosiTheme.radiusMd
                            color: KodosiTheme.alpha(KodosiTheme.accentSoft, 0.7)
                        }
                        HoverHandler { id: hover }
                        RowLayout {
                            id: row
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.leftMargin: entry.mentionsMe ? 8 : 0
                            anchors.rightMargin: entry.mentionsMe ? 8 : 0
                            y: entry.mentionsMe ? 6 : 0
                            spacing: 10

                            Item {
                                Layout.alignment: Qt.AlignTop
                                Layout.preferredWidth: 28
                                Layout.preferredHeight: entry.grouped ? 1 : 28
                                visible: !entry.own

                                AgentMark { visible: entry.isAgent && !entry.grouped; program: entry.modelData.agent || ""; size: 28 }
                                PersonAvatar { visible: !entry.isAgent && !entry.grouped; name: entry.modelData.authorName; key: entry.modelData.authorId; size: 28 }
                            }
                            Item { visible: entry.own; Layout.fillWidth: true; Layout.minimumWidth: 44 }
                            ColumnLayout {
                                Layout.fillWidth: !entry.own
                                Layout.maximumWidth: entry.own ? entry.width - 44 : -1
                                spacing: 3

                                RowLayout {
                                    visible: !entry.own && !entry.grouped
                                    spacing: 6
                                    PlainLabel { text: entry.isAgent ? entry.modelData.agent : entry.modelData.authorName; font.weight: Font.DemiBold }
                                    PlainLabel { visible: entry.isAgent; text: qsTr("for %1").arg(entry.modelData.authorName); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption }
                                    AbstractButton {
                                        id: chip
                                        visible: entry.terminal !== null
                                        implicitHeight: 18
                                        implicitWidth: chipLabel.implicitWidth + 14
                                        hoverEnabled: true
                                        Accessible.name: qsTr("Open %1").arg(entry.terminal ? entry.terminal.name : "")
                                        contentItem: PlainLabel {
                                            id: chipLabel
                                            text: entry.terminal ? entry.terminal.name : ""
                                            color: KodosiTheme.accentStrong
                                            font.pixelSize: KodosiTheme.fontCaption2
                                            font.weight: Font.DemiBold
                                            horizontalAlignment: Text.AlignHCenter
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        background: Rectangle { radius: 9; color: chip.hovered ? KodosiTheme.mix(KodosiTheme.accentSoft, KodosiTheme.accent, 0.2) : KodosiTheme.accentSoft }
                                        onClicked: root.terminalRequested(entry.modelData.terminalId)
                                    }
                                    PlainLabel { text: Qt.formatTime(entry.sent, "hh:mm"); color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption2 }
                                    Item { Layout.fillWidth: true }
                                }
                                Rectangle {
                                    Layout.fillWidth: !entry.own
                                    Layout.alignment: entry.own ? Qt.AlignRight : Qt.AlignLeft
                                    implicitWidth: Math.min(body.implicitWidth + (entry.own ? 24 : 0), entry.width - 44)
                                    implicitHeight: body.implicitHeight + (entry.own ? 14 : 0)
                                    radius: 16
                                    color: entry.own ? KodosiTheme.accentSoft : "transparent"

                                    TextEdit {
                                        id: body
                                        anchors.fill: parent
                                        anchors.leftMargin: entry.own ? 12 : 0
                                        anchors.rightMargin: entry.own ? 12 : 0
                                        anchors.topMargin: entry.own ? 7 : 0
                                        text: Mentions.html(entry.modelData.text, root.targets, entry.own || entry.mentionsMe ? KodosiTheme.mix(KodosiTheme.accentSoft, KodosiTheme.accent, 0.28) : KodosiTheme.accentSoft, KodosiTheme.accentStrong)
                                        color: KodosiTheme.ink
                                        font.pixelSize: KodosiTheme.fontBody
                                        readOnly: true
                                        selectByMouse: true
                                        selectionColor: KodosiTheme.accent
                                        selectedTextColor: KodosiTheme.accentInk
                                        wrapMode: TextEdit.Wrap
                                        textFormat: TextEdit.RichText
                                        Accessible.name: entry.modelData.text
                                        onLinkActivated: link => {
                                            if (link.startsWith("kodosi-mention://terminal/"))
                                                root.terminalRequested(link.substring(26));
                                        }

                                        HoverHandler { cursorShape: body.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor }
                                    }
                                }
                            }
                        }
                        Row {
                            anchors.right: parent.right
                            anchors.rightMargin: entry.own ? 0 : 4
                            anchors.top: parent.top
                            anchors.topMargin: -12
                            visible: hover.hovered
                            spacing: 0
                            z: 2

                            Raised { width: actions.width + 6; height: 26; radius: 13; fill: KodosiTheme.lifted; elevation: 1
                                Row {
                                    id: actions
                                    x: 3
                                    KIconButton {
                                        size: 26
                                        glyph: "copy"
                                        Accessible.name: qsTr("Copy")
                                        ToolTip.visible: hovered
                                        ToolTip.text: qsTr("Copy")
                                        onClicked: { body.selectAll(); body.copy(); body.deselect(); }
                                    }
                                    KIconButton {
                                        size: 26
                                        glyph: "tasks"
                                        Accessible.name: qsTr("Make this a task")
                                        ToolTip.visible: hovered
                                        ToolTip.text: qsTr("Make this a task")
                                        enabled: !Models.Missions.busy
                                        onClicked: Models.Missions.roomAction({ type: "createTask", title: entry.modelData.text.split("\n")[0].slice(0, 120), description: entry.modelData.text, repositoryIds: [] })
                                    }
                                }
                            }
                        }
                    }
                }
                add: Transition {
                    NumberAnimation { properties: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade }
                }
            }
            KButton {
                anchors.bottom: parent.bottom
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottomMargin: 8
                visible: !transcript.atYEnd && root.messages.length > 0
                compact: true
                text: qsTr("Latest")
                iconName: "chevron-down"
                onClicked: { transcript.positionViewAtEnd(); Models.Missions.setPresentation("followLatest", true); }
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 6
            visible: root.suggestions.length > 0
            implicitHeight: list.implicitHeight + 10

            Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; fill: KodosiTheme.lifted; elevation: 2 }
            Column {
                id: list
                x: 5
                y: 5
                width: parent.width - 10
                spacing: 1

                Repeater {
                    model: root.suggestions

                    AbstractButton {
                        id: suggestion

                        required property var modelData
                        required property int index
                        readonly property bool current: index === Math.min(root.highlighted, root.suggestions.length - 1)

                        width: list.width
                        height: 32
                        hoverEnabled: true
                        objectName: "room.mention." + modelData.id
                        Accessible.id: objectName
                        Accessible.name: modelData.name
                        contentItem: RowLayout {
                            spacing: 9
                            Item {
                                Layout.leftMargin: 8
                                Layout.preferredWidth: 22
                                Layout.preferredHeight: 22
                                PersonAvatar { visible: suggestion.modelData.kind === "person"; name: suggestion.modelData.name; key: suggestion.modelData.id; size: 22 }
                                AgentMark { visible: suggestion.modelData.kind === "terminal"; program: suggestion.modelData.program; size: 22 }
                            }
                            PlainLabel { text: suggestion.modelData.name; font.weight: Font.Medium }
                            PlainLabel { Layout.fillWidth: true; text: suggestion.modelData.detail; color: KodosiTheme.inkFaint; font.pixelSize: KodosiTheme.fontCaption; elide: Text.ElideRight }
                        }
                        background: Rectangle {
                            radius: KodosiTheme.radiusSm
                            color: suggestion.current ? KodosiTheme.accentSoft : KodosiTheme.alpha(KodosiTheme.ink, suggestion.hovered ? 0.05 : 0)
                        }
                        onClicked: { root.pick(modelData); editor.forceActiveFocus(); }
                    }
                }
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 10
            Layout.topMargin: 4
            implicitHeight: Math.min(150, Math.max(40, editor.implicitHeight + 10))

            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                radius: 23
                color: KodosiTheme.alpha(KodosiTheme.accent, editor.activeFocus ? 0.16 : 0)
                Behavior on color { ColorAnimation { duration: KodosiTheme.motionFade } }
            }
            Raised { anchors.fill: parent; radius: 20; fill: KodosiTheme.lifted; elevation: editor.activeFocus ? 2 : 1 }
            Rectangle {
                anchors.fill: parent
                radius: 20
                color: "transparent"
                border.width: 1.5
                border.color: KodosiTheme.alpha(KodosiTheme.accent, editor.activeFocus ? 0.55 : 0)
                Behavior on border.color { ColorAnimation { duration: KodosiTheme.motionFade } }
            }
            RowLayout {
                anchors.fill: parent
                anchors.margins: 5
                spacing: 4

                KIconButton {
                    Layout.alignment: Qt.AlignBottom
                    size: 30
                    glyph: "at"
                    Accessible.name: qsTr("Mention someone or a terminal")
                    objectName: "room.message.mention"
                    Accessible.id: objectName
                    onClicked: {
                        root.dismissed = "";
                        if (Mentions.query(editor.text).length === 0)
                            Models.Missions.setPresentation("message", editor.text + (editor.text.length === 0 || editor.text.endsWith(" ") ? "@" : " @"));
                        editor.forceActiveFocus();
                        editor.cursorPosition = editor.text.length;
                    }
                }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    TextArea {
                        id: editor
                        text: root.presentation.message || ""
                        onTextChanged: {
                            if (text !== (root.presentation.message || ""))
                                Models.Missions.setPresentation("message", text);
                        }
                        placeholderText: qsTr("Message the room")
                        color: KodosiTheme.ink
                        placeholderTextColor: KodosiTheme.inkFaint
                        selectionColor: KodosiTheme.accent
                        selectedTextColor: KodosiTheme.accentInk
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        font.pixelSize: KodosiTheme.fontCallout
                        topPadding: 6
                        bottomPadding: 6
                        leftPadding: 2
                        background: null
                        objectName: "room.message.input"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Message the room")
                        Keys.onPressed: event => {
                            const choosing = root.suggestions.length > 0;
                            if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                                if (choosing)
                                    root.pick(root.suggestions[Math.min(root.highlighted, root.suggestions.length - 1)]);
                                else if (root.canSend)
                                    root.send();
                                event.accepted = true;
                            } else if (choosing && event.key === Qt.Key_Tab) {
                                root.pick(root.suggestions[Math.min(root.highlighted, root.suggestions.length - 1)]);
                                event.accepted = true;
                            } else if (choosing && (event.key === Qt.Key_Down || event.key === Qt.Key_Up)) {
                                const count = root.suggestions.length;
                                root.highlighted = (Math.min(root.highlighted, count - 1) + (event.key === Qt.Key_Down ? 1 : -1) + count) % count;
                                event.accepted = true;
                            } else if (choosing && event.key === Qt.Key_Escape) {
                                root.dismissed = root.query;
                                event.accepted = true;
                            }
                        }
                    }
                }
                AbstractButton {
                    id: sendButton
                    Layout.alignment: Qt.AlignBottom
                    implicitWidth: 30
                    implicitHeight: 30
                    enabled: root.canSend
                    scale: pressed ? 0.9 : 1
                    Accessible.name: qsTr("Send message")
                    objectName: "room.message.send"
                    Accessible.id: objectName
                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }
                    background: Item {
                        Raised { anchors.fill: parent; radius: 15; fill: KodosiTheme.accent; visible: root.canSend }
                        Rectangle { anchors.fill: parent; radius: 15; color: KodosiTheme.alpha(KodosiTheme.ink, 0.08); visible: !root.canSend }
                    }
                    contentItem: Item {
                        KIcon { anchors.centerIn: parent; width: 14; height: 14; name: "send"; strokeWidth: 2.6; color: root.canSend ? KodosiTheme.accentInk : KodosiTheme.inkFaint; visible: !Models.Missions.busy }
                        CursorBlock { anchors.centerIn: parent; width: 6; height: 12; blinks: true; visible: Models.Missions.busy }
                    }
                    onClicked: root.send()
                }
            }
        }
    }
}
