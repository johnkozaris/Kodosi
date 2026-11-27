import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

FocusScope {
    id: root

    signal focused
    signal terminalRequested
    signal linkQuestionRequested(url link, string target)
    signal linkRefused(string target, string reason)

    required property string sessionId
    required property var session
    required property Look look
    property string otherComputer: ""
    property Models.TerminalChat watched: null
    readonly property Models.TerminalChat chat: Models.Chats.chat(sessionId)
    readonly property string agent: Models.DesktopSettings.agentLabel(session.program || "")
    readonly property string writer: chat.facts.agent || session.program || ""

    function focusComposer(): void {
        dock.focusComposer()
    }
    function follow(): void {
        if (watched === chat)
            return
        if (watched)
            watched.unwatch()
        watched = chat
        if (watched)
            watched.watch()
    }
    function open(link: string): void {
        const terminal = Mentions.terminal(link)
        if (terminal.length > 0)
            Models.SessionActions.activate(terminal)
        else
            root.chat.follow(link, root.otherComputer)
    }
    function peer(terminal: string): var {
        return Mentions.peerTargets(root.chat.peers).find(target => target.id === terminal) || null
    }

    objectName: "chat." + sessionId
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Chat")

    onActiveFocusChanged: {
        if (activeFocus)
            focused()
    }
    onChatChanged: follow()
    Component.onCompleted: follow()
    Component.onDestruction: {
        if (watched)
            watched.unwatch()
    }

    Connections {
        function onLinkQuestionRequested(link: url, target: string) {
            root.linkQuestionRequested(link, target)
        }
        function onLinkRefused(target: string, reason: string) {
            root.linkRefused(target, reason)
        }

        target: root.chat
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        ChatThread {
            id: thread

            Layout.fillWidth: true
            Layout.fillHeight: true
            chat: root.chat
            session: root.session
            look: root.look
            agent: Models.DesktopSettings.agentLabel(root.writer)
            program: root.writer

            onTapped: root.focused()
            onLinkActivated: link => root.open(link)
        }
        ChatDock {
            id: dock

            Layout.fillWidth: true
            Layout.maximumHeight: root.height
            chat: root.chat
            session: root.session
            sessionId: root.sessionId
            look: root.look
            agent: root.agent
            room: root.height
            onSent: thread.toEnd(true)
            onTerminalRequested: root.terminalRequested()
            onLinkActivated: link => root.open(link)
        }
    }
    TerminalDropArea {
        id: chatDrop

        anchors.fill: parent
        anchors.margins: 6
        radius: KodosiTheme.radiusLg
        enabled: root.chat.canWrite && root.chat.peers.length > 0
        objectName: root.objectName + ".drop"

        onEntered: drag => drag.accepted = root.peer(drag.getDataAsString("application/x-kodosi-terminal")) !== null
        onTerminalDropped: (id, at) => {
            dropMenu.target = root.peer(id)
            if (dropMenu.target)
                Qt.callLater(() => dropMenu.popup(chatDrop, at.x, at.y))
        }
    }
    ChatPeerMenu {
        id: dropMenu

        titled: true
        identifier: "chat.drop"
        onMentionChosen: target => dock.mention(target)
        onHandOffChosen: target => {
            root.chat.handOff(target.id)
            thread.toEnd(true)
        }
    }
}
