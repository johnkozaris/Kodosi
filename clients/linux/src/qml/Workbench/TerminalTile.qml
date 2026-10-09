pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property bool roomEmbedded: false
    property bool accessibilitySuppressed: false
    property bool interactionEnabled: true
    readonly property bool active: Models.DesktopState.selectedSessionId === sessionId
    readonly property bool ringed: active && !roomEmbedded
    property bool componentReady: false
    property bool focusedSizeAuthority: false
    required property string sessionId
    property var session: ({})
    readonly property string sessionName: session.name || qsTr("Terminal")
    readonly property bool shareable: session.kind === "local" && session.isOwner === true
    readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)
    readonly property string notice: session.kind === "remote" && session.connectionState === "connected"
        ? (session.status === "reconnecting" ? qsTr("Reconnecting") : session.message || "") : ""
    property string terminalError: ""
    property string terminalOperationError: ""
    property string joinedUser: ""

    signal inspectSessionRequested(string sessionId, string sessionName)
    signal shareSessionRequested(string sessionId, string sessionName)

    function bindTerminal() {
        terminalError = "";
        terminalOperationError = "";
        if (!Models.TerminalSurfaces.bind(terminal, sessionId))
            terminalError = qsTr("This terminal is not available now.");
    }
    function forceTerminalFocus() {
        if (visible && enabled)
            terminal.forceActiveFocus(Qt.ShortcutFocusReason);
    }
    function refreshSession() {
        const before = viewers;
        session = Models.Sessions.presentationForSession(sessionId);
        const added = viewers.find(user => before.indexOf(user) < 0);
        if (added && componentReady) {
            joinedUser = added;
            join.restart();
        }
    }
    function seen() {
        if (visible && !accessibilitySuppressed && Window.active)
            Models.Sessions.clearAttention(sessionId);
    }
    function select() {
        Models.DesktopState.selectSession(sessionId);
        Models.Sessions.clearAttention(sessionId);
    }

    Accessible.id: objectName
    Accessible.ignored: accessibilitySuppressed || !visible
    Accessible.name: qsTr("%1 terminal").arg(sessionName)
    Accessible.role: Accessible.Pane
    objectName: "stage.tile." + sessionId

    Component.onCompleted: {
        refreshSession();
        componentReady = true;
        bindTerminal();
        seen();
        if (Models.SessionActions.takeCreated(sessionId) && !KodosiTheme.reduceMotion)
            birth.start();
    }
    Component.onDestruction: Models.TerminalSurfaces.detach(terminal)
    onSessionIdChanged: {
        if (componentReady) {
            Models.TerminalSurfaces.detach(terminal);
            refreshSession();
            bindTerminal();
        }
    }
    onVisibleChanged: seen()

    Connections {
        function onModelReset() {
            root.refreshSession();
        }
        function onAttentionChanged() {
            root.seen();
        }

        target: Models.Sessions
    }
    Connections {
        function onAttachmentReady(surface, id) {
            if (surface === terminal && id === root.sessionId)
                root.terminalError = "";
        }
        function onAttachmentRejected(surface, id, reason) {
            if (surface === terminal && id === root.sessionId)
                root.terminalError = reason;
        }

        target: Models.TerminalSurfaces
    }
    HoverHandler { id: tileHover }
    Raised {
        anchors.fill: parent
        radius: KodosiTheme.radiusTile
        fill: KodosiTheme.terminal
        rim: false
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 36
            topLeftRadius: KodosiTheme.radiusTile
            topRightRadius: KodosiTheme.radiusTile
            color: root.active ? KodosiTheme.terminalBandSelected : KodosiTheme.terminalBand

            TapHandler { onTapped: root.select() }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 9
                anchors.rightMargin: 6
                spacing: 8

                AgentMark {
                    program: root.session.program || ""
                    size: 20
                    session: root.session
                }
                PlainLabel {
                    Layout.maximumWidth: Math.max(80, root.width * 0.4)
                    text: root.sessionName
                    color: KodosiTheme.terminalInk
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                ActivityLine {
                    Layout.fillWidth: true
                    session: root.session
                    color: KodosiTheme.terminalInkMuted
                }
                PlainLabel {
                    Accessible.id: objectName
                    color: KodosiTheme.terminalInkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                    objectName: root.objectName + ".notice"
                    text: root.notice
                    visible: root.notice.length > 0
                }
                AvatarStack {
                    visible: root.viewers.length > 0
                    userIds: root.viewers
                    size: 20
                    limit: 3
                    ring: KodosiTheme.terminalBand
                }
                Row {
                    spacing: 0
                    opacity: tileHover.hovered || root.active ? 1 : 0.35

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                    Repeater {
                        model: root.session.atPrompt === true ? Models.DesktopSettings.startCommands : []

                        delegate: StartMark {
                            required property var modelData

                            command: modelData
                            onTerminal: true
                            objectName: root.objectName + ".start." + modelData.id
                            Accessible.id: objectName
                            onClicked: Models.SessionActions.run(root.sessionId, modelData.command)
                        }
                    }
                    KIconButton {
                        glyph: "warning"
                        size: 26
                        onTerminal: true
                        visible: root.terminalOperationError.length > 0
                        glyphColor: KodosiTheme.caution
                        Accessible.name: qsTr("Terminal action failed")
                        onClicked: operationFailure.open()
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: qsTr("Share")
                        glyph: "share"
                        size: 26
                        onTerminal: true
                        objectName: root.objectName + ".share"
                        visible: root.shareable
                        active: !!root.session.missionId || (root.session.sharedWith || []).length > 0
                        onClicked: root.shareSessionRequested(root.sessionId, root.sessionName)
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: qsTr("Details")
                        glyph: "info"
                        size: 26
                        onTerminal: true
                        objectName: root.objectName + ".details"
                        onClicked: root.inspectSessionRequested(root.sessionId, root.sessionName)
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: qsTr("Minimize")
                        glyph: "minus"
                        size: 26
                        onTerminal: true
                        objectName: root.objectName + ".minimize"
                        onClicked: Models.SessionActions.minimize(root.sessionId)
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: root.focusedSizeAuthority ? qsTr("Return to tiles") : qsTr("Zoom")
                        glyph: "focus"
                        size: 26
                        onTerminal: true
                        active: root.focusedSizeAuthority
                        objectName: root.objectName + ".focus"
                        visible: !root.roomEmbedded

                        onClicked: {
                            Models.DesktopState.selectSession(root.sessionId);
                            Models.DesktopState.toggleFocusForSelectedSession();
                        }
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: qsTr("Close terminal")
                        glyph: "close"
                        size: 26
                        onTerminal: true
                        destructive: true
                        objectName: root.objectName + ".close"
                        onClicked: closeConfirmation.open()
                    }
                }
            }
        }
        Item {
            Layout.fillHeight: true
            Layout.fillWidth: true
            Layout.topMargin: 3
            Layout.leftMargin: 5
            Layout.rightMargin: 5
            Layout.bottomMargin: 5

            Models.TerminalView {
                id: terminal
                enabled: root.interactionEnabled

                Accessible.id: objectName
                anchors.fill: parent
                cursorBlink: Models.DesktopSettings.cursorBlink
                cursorStyle: Models.DesktopSettings.cursorStyle
                focusedSizeAuthority: root.focusedSizeAuthority
                fontFamily: Models.DesktopSettings.fontFamily
                fontPixelSize: Models.DesktopSettings.fontSize
                lineHeight: Models.DesktopSettings.lineHeight
                objectName: root.objectName + ".terminal"
                preeditBackground: KodosiTheme.terminalAccent
                preeditForeground: "#160e0a"
                scrollbackLines: Models.DesktopSettings.scrollbackLines
                selectionBackground: KodosiTheme.terminalAccent
                selectionForeground: "#160e0a"

                onActiveFocusChanged: {
                    if (activeFocus)
                        root.select();
                }
                onContextMenuRequested: (x, y) => {
                    const point = terminal.mapToItem(root, x, y);
                    context.popup(point.x, point.y);
                }
                onOperationError: message => root.terminalOperationError = message
                onTerminalClosed: root.terminalError = qsTr("The terminal closed.")
                onTerminalError: message => root.terminalError = message
            }
            Rectangle {
                anchors.fill: parent
                color: KodosiTheme.terminal
                visible: !terminal.terminalReady || root.terminalError.length > 0

                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 14
                    width: Math.min(320, parent.width - 32)

                    CursorBlock {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 12
                        Layout.preferredHeight: 22
                        visible: root.terminalError.length === 0
                        blinks: visible
                        color: KodosiTheme.terminalAccent
                    }
                    ShimmerText {
                        Layout.alignment: Qt.AlignHCenter
                        visible: root.terminalError.length === 0
                        text: qsTr("Connecting")
                        active: visible
                        color: KodosiTheme.terminalInk
                        font.pixelSize: KodosiTheme.fontHeadline
                        font.weight: Font.DemiBold
                        Accessible.ignored: false
                        Accessible.name: qsTr("Connecting terminal")
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.terminalInk
                        font.pixelSize: KodosiTheme.fontHeadline
                        font.weight: Font.DemiBold
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("This terminal is not available")
                        visible: root.terminalError.length > 0
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.terminalInkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                        horizontalAlignment: Text.AlignHCenter
                        text: root.terminalError
                        visible: root.terminalError.length > 0
                        wrapMode: Text.WordWrap
                    }
                    KButton {
                        Accessible.id: objectName
                        Layout.alignment: Qt.AlignHCenter
                        objectName: root.objectName + ".retry"
                        text: qsTr("Try again")
                        variant: KButton.Primary
                        visible: root.terminalError.length > 0

                        onClicked: {
                            root.terminalError = "";
                            const activated = root.roomEmbedded ? Models.SessionActions.activateInRoom(root.sessionId) : Models.SessionActions.activate(root.sessionId);
                            if (activated && !Models.TerminalSurfaces.retry(terminal, root.sessionId))
                                root.terminalError = qsTr("The terminal could not connect again.");
                        }
                    }
                }
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: KodosiTheme.radiusTile
        color: "transparent"
        border.width: root.ringed ? 1.5 : 1
        border.color: root.ringed ? KodosiTheme.alpha(KodosiTheme.terminalAccent, 0.8) : KodosiTheme.alpha(KodosiTheme.terminalHairline, 0.8)

        Behavior on border.color { ColorAnimation { duration: KodosiTheme.motionFade } }
    }
    Rectangle {
        id: veil
        anchors.fill: parent
        radius: KodosiTheme.radiusTile
        color: KodosiTheme.terminalAccent
        opacity: 0
        visible: opacity > 0
        transformOrigin: Item.TopLeft

        ParallelAnimation {
            id: birth
            NumberAnimation { target: veil; property: "scale"; from: 0.04; to: 1; duration: 380; easing.type: Easing.OutCubic }
            SequentialAnimation {
                NumberAnimation { target: veil; property: "opacity"; from: 0.85; to: 0.85; duration: 90 }
                NumberAnimation { target: veil; property: "opacity"; to: 0; duration: 420; easing.type: Easing.OutCubic }
            }
        }
    }
    Item {
        id: joinCard
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: 46
        anchors.rightMargin: 12
        width: joinRow.implicitWidth + 24
        height: 44
        opacity: 0
        visible: opacity > 0

        Raised {
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
            fill: "#2e241f"
            elevation: 2
        }
        RowLayout {
            id: joinRow
            anchors.verticalCenter: parent.verticalCenter
            x: 9
            spacing: 9

            PersonAvatar { name: Identity.personName(root.joinedUser); key: root.joinedUser; size: 26 }
            ColumnLayout {
                spacing: 1
                PlainLabel { text: qsTr("%1 joined").arg(Identity.personName(root.joinedUser)); color: KodosiTheme.terminalInk; font.weight: Font.DemiBold }
                PlainLabel { text: qsTr("Full control"); color: KodosiTheme.terminalInkMuted; font.pixelSize: KodosiTheme.fontCaption }
            }
        }
        SequentialAnimation {
            id: join
            NumberAnimation { target: joinCard; property: "opacity"; to: 1; duration: KodosiTheme.motionFade }
            PauseAnimation { duration: 3200 }
            NumberAnimation { target: joinCard; property: "opacity"; to: 0; duration: 500 }
        }
    }
    KPopover {
        id: operationFailure
        x: Math.max(0, root.width - width - 8)
        y: 40
        width: Math.min(360, root.width - 16)
        padding: 16
        contentItem: ColumnLayout {
            spacing: 12
            KReadOnlyText { Layout.fillWidth: true; text: root.terminalOperationError; objectName: root.objectName + ".operationError" }
            KButton {
                text: qsTr("Dismiss")
                compact: true
                objectName: root.objectName + ".operationError.dismiss"
                onClicked: { root.terminalOperationError = ""; operationFailure.close(); }
            }
        }
    }
    KMenu {
        id: context

        KMenuItem {
            Accessible.id: objectName
            enabled: terminal.hasSelection
            iconName: "copy"
            objectName: "panel.terminalTile.copy"
            text: qsTr("Copy")

            onTriggered: terminal.copySelectionToClipboard()
        }
        KMenuItem {
            Accessible.id: objectName
            enabled: terminal.terminalReady
            iconName: "document"
            objectName: "panel.terminalTile.paste"
            text: qsTr("Paste")

            onTriggered: terminal.pasteFromClipboard()
        }
    }
    KDialog {
        id: closeConfirmation

        destructive: true
        objectName: root.objectName + ".closeConfirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Close %1?").arg(root.sessionName)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Close")

        onAccepted: Models.SessionActions.close(root.sessionId)

        PlainLabel {
            color: KodosiTheme.inkMuted
            width: 340
            wrapMode: Text.WordWrap
            text: qsTr("Its programs stop.")
        }
    }
}
