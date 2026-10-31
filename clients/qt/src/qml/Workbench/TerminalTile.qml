pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property bool roomEmbedded: false
    property bool joined: false
    property bool canvasOpen: false
    property bool accessibilitySuppressed: false
    property bool interactionEnabled: true
    readonly property bool active: Models.DesktopState.selectedSessionId === sessionId
    readonly property bool ringed: active && !roomEmbedded
    property bool componentReady: false
    readonly property bool watched: componentReady && visible && !accessibilitySuppressed && Window.active
    property string watchedId: ""
    property bool zoomed: false
    required property string sessionId
    property var session: ({})
    readonly property string sessionName: session.name || qsTr("Terminal")
    readonly property bool shareable: session.kind === "local" && session.isOwner === true
    readonly property var viewers: (session.connectedUsers || []).filter(user => user !== Models.Account.userId)
    readonly property bool away: session.kind === "remote" && session.status === "offline"
    readonly property string awayText: qsTr("%1 is offline").arg(session.hostLabel || qsTr("Remote computer"))
    readonly property string notice: session.kind === "remote" && session.connectionState === "connected"
        ? (away ? awayText : session.status === "reconnecting" ? qsTr("Reconnecting") : session.message || "") : ""
    property string terminalError: ""
    property bool connectingLong: false
    property string terminalOperationError: ""
    property string joinedUser: ""

    signal inspectSessionRequested(string sessionId)
    signal shareSessionRequested(string sessionId)
    signal canvasRequested(string section)

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
    function reportWatched() {
        const id = watched ? sessionId : "";
        if (id === watchedId)
            return;
        if (watchedId !== "")
            Models.Sessions.setVisible(watchedId, false);
        watchedId = id;
        if (id !== "")
            Models.Sessions.setVisible(id, true);
    }
    function select() {
        Models.DesktopState.selectSession(sessionId);
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
        if (Models.SessionActions.takeCreated(sessionId) && !KodosiTheme.reduceMotion)
            birth.start();
    }
    Component.onDestruction: {
        if (watchedId !== "")
            Models.Sessions.setVisible(watchedId, false);
        Models.TerminalSurfaces.detach(terminal);
    }
    onSessionIdChanged: {
        if (componentReady) {
            Models.TerminalSurfaces.detach(terminal);
            refreshSession();
            bindTerminal();
        }
        reportWatched();
    }
    onWatchedChanged: reportWatched()

    Connections {
        function onModelReset() {
            root.refreshSession();
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
        visible: !root.joined
        radius: KodosiTheme.radiusTile
        fill: KodosiTheme.terminal
        rim: false
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: root.joined ? 38 : 36
            topLeftRadius: KodosiTheme.radiusTile
            topRightRadius: KodosiTheme.radiusTile
            color: root.joined ? "transparent" : root.active ? KodosiTheme.terminalBandSelected : KodosiTheme.terminalBand

            TapHandler { onTapped: root.select() }
            TerminalDragSource {
                anchors.fill: parent
                enabled: !root.roomEmbedded
                sessionId: root.sessionId
            }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: root.roomEmbedded ? 11 : 9
                anchors.rightMargin: root.canvasOpen ? 16 : 6
                spacing: 8

                AgentMark {
                    visible: !root.roomEmbedded
                    program: root.session.program || ""
                    size: 20
                    session: root.session
                }
                PlainLabel {
                    Layout.maximumWidth: Math.max(80, root.width * 0.4)
                    visible: !root.roomEmbedded
                    text: root.sessionName
                    color: KodosiTheme.terminalInk
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                RowLayout {
                    id: location

                    readonly property string host: root.roomEmbedded || root.session.kind === "remote" ? root.session.hostLabel || "" : ""
                    readonly property string place: root.roomEmbedded ? root.session.folderName || "" : root.session.placeName || ""
                    readonly property string placeGlyph: root.roomEmbedded ? "folder" : root.session.placeKind === "container" ? "container"
                        : root.session.placeKind === "vm" ? "cube" : "server"

                    Layout.maximumWidth: Math.max(80, root.width * 0.45)
                    visible: host.length > 0 || place.length > 0
                    spacing: 4
                    Accessible.role: Accessible.StaticText
                    Accessible.name: host.length > 0 && place.length > 0 ? qsTr("%1 on %2").arg(place).arg(host) : host + place
                    ToolTip.text: root.roomEmbedded ? root.session.displayDirectory || host : root.session.placeLabel || host
                    ToolTip.visible: locationHover.hovered && ToolTip.text.length > 0
                    ToolTip.delay: Tip.delay

                    HoverHandler { id: locationHover }
                    KIcon {
                        Layout.preferredWidth: 11
                        Layout.preferredHeight: 11
                        visible: location.host.length > 0
                        name: "laptop"
                        strokeWidth: 2.2
                        color: KodosiTheme.terminalInkMuted
                    }
                    PlainLabel {
                        visible: location.host.length > 0
                        text: location.host
                        color: KodosiTheme.terminalInkMuted
                        font.pixelSize: KodosiTheme.fontCaption
                        font.weight: Font.Medium
                    }
                    KIcon {
                        Layout.preferredWidth: 13
                        Layout.preferredHeight: 13
                        Layout.leftMargin: -2
                        Layout.rightMargin: -2
                        visible: location.host.length > 0 && location.place.length > 0
                        name: "chevron-compact-right"
                        strokeWidth: 2.2
                        color: KodosiTheme.terminalInkFaint
                    }
                    KIcon {
                        Layout.preferredWidth: 11
                        Layout.preferredHeight: 11
                        visible: location.place.length > 0
                        name: location.placeGlyph
                        strokeWidth: 2.2
                        color: KodosiTheme.mix(KodosiTheme.terminalInk, KodosiTheme.terminalInkMuted, 0.35)
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        visible: location.place.length > 0
                        text: location.place
                        color: KodosiTheme.mix(KodosiTheme.terminalInk, KodosiTheme.terminalInkMuted, 0.35)
                        font.pixelSize: KodosiTheme.fontCaption
                        font.weight: Font.Medium
                        elide: Text.ElideMiddle
                    }
                }
                ActivityLine {
                    Layout.fillWidth: true
                    session: root.session
                    color: KodosiTheme.terminalInkMuted
                }
                DosiKeys {
                    sessionId: root.sessionId
                }
                StatusSign {
                    session: root.session
                    size: 12
                }
                PlainLabel {
                    Accessible.id: objectName
                    color: root.away ? KodosiTheme.caution : KodosiTheme.terminalInkFaint
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
                CanvasDock {
                    sessionId: root.sessionId
                    identifier: root.objectName + ".canvas"
                    visible: !!root.session.canvas && !root.canvasOpen
                    onPressed: section => root.canvasRequested(section)
                }
                Row {
                    spacing: 0
                    opacity: tileHover.hovered || root.active && !root.joined ? 1 : 0.35

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                    AbstractButton {
                        id: actionsKey

                        implicitWidth: 26
                        implicitHeight: 26
                        hoverEnabled: true
                        activeFocusOnTab: true
                        scale: pressed ? 0.9 : 1
                        objectName: root.objectName + ".actions"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Actions")
                        ToolTip.text: Accessible.name
                        ToolTip.visible: hovered && !actionsMenu.visible
                        ToolTip.delay: Tip.delay

                        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                        contentItem: Item {
                            KIcon {
                                anchors.centerIn: parent
                                width: 14
                                height: 14
                                name: "sliders"
                                strokeWidth: 1.9
                                color: KodosiTheme.terminalAccentStrong
                            }
                        }
                        background: Rectangle {
                            radius: KodosiTheme.radiusXs
                            color: actionsKey.hovered || actionsMenu.visible ? KodosiTheme.terminalAccentSoft : "transparent"
                            border.width: actionsKey.visualFocus ? 1.5 : 0
                            border.color: KodosiTheme.terminalAccentStrong

                            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                        }
                        onClicked: actionsMenu.popup(actionsKey, actionsKey.width - actionsMenu.width, actionsKey.height + 4)

                        KMenu {
                            id: actionsMenu

                            readonly property var agents: root.session.atPrompt === true ? Models.Launcher.agents.filter(agent => agent.installed) : []

                            PlainLabel {
                                visible: actionsMenu.agents.length > 0
                                height: visible ? implicitHeight : 0
                                leftPadding: 10
                                topPadding: 4
                                bottomPadding: 2
                                text: qsTr("Start an agent")
                                color: KodosiTheme.inkFaint
                                font.pixelSize: KodosiTheme.fontCaption
                                font.weight: Font.Medium
                            }
                            Instantiator {
                                model: actionsMenu.agents
                                onObjectAdded: (index, item) => actionsMenu.insertItem(index + 1, item)
                                onObjectRemoved: (index, item) => actionsMenu.removeItem(item)
                                delegate: KMenuItem {
                                    required property var modelData

                                    text: Models.DesktopSettings.agentLabel(modelData.agent)
                                    agent: modelData.agent
                                    objectName: root.objectName + ".start." + modelData.agent
                                    onTriggered: Models.SessionActions.startAgent(root.sessionId, modelData.agent)
                                }
                            }
                            MenuSeparator {
                                visible: actionsMenu.agents.length > 0
                                height: visible ? implicitHeight : 0
                                contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
                            }
                            KMenuItem {
                                text: qsTr("Share…")
                                iconName: "share"
                                visible: root.shareable
                                objectName: root.objectName + ".share"
                                onTriggered: root.shareSessionRequested(root.sessionId)
                            }
                            KMenuItem {
                                text: qsTr("Open Browser")
                                iconName: "globe"
                                visible: root.session.canControl === true
                                objectName: root.objectName + ".browser"
                                onTriggered: {
                                    Models.Canvas.openBrowser(root.sessionId);
                                    root.canvasRequested("");
                                }
                            }
                            KMenuItem {
                                text: qsTr("Details")
                                iconName: "info"
                                objectName: root.objectName + ".details"
                                onTriggered: root.inspectSessionRequested(root.sessionId)
                            }
                            MenuSeparator {
                                visible: root.roomEmbedded && root.session.isOwner === true
                                height: visible ? implicitHeight : 0
                                contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
                            }
                            KMenuItem {
                                text: qsTr("Remove from room")
                                iconName: "leave"
                                destructive: true
                                visible: root.roomEmbedded && root.session.isOwner === true
                                objectName: root.objectName + ".removeFromRoom"
                                onTriggered: Models.SessionActions.attachMission(root.sessionId, "")
                            }
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
                        Accessible.name: qsTr("Minimize")
                        glyph: "minus"
                        size: 26
                        onTerminal: true
                        visible: !root.canvasOpen
                        objectName: root.objectName + ".minimize"
                        onClicked: Models.SessionActions.minimize(root.sessionId)
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: root.zoomed ? qsTr("Return to tiles") : qsTr("Zoom")
                        glyph: "focus"
                        size: 26
                        onTerminal: true
                        active: root.zoomed
                        objectName: root.objectName + ".focus"
                        visible: !root.roomEmbedded && !root.canvasOpen

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
                        onClicked: closeConfirmation.ask(root.sessionId, root.sessionName)
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
                asksBeforeClipboardRead: root.session.isOwner !== true
                otherComputer: root.session.kind === "local" ? "" : (root.session.hostLabel || "")
                cursorBlink: Models.DesktopSettings.cursorBlink
                cursorStyle: Models.DesktopSettings.cursorStyle
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
                onClipboardReadRequested: clipboardRead.open()
                onLinkQuestionRequested: (link, target) => linkReview.ask(link, target)
                onLinkRefused: (target, reason) => linkReview.refuse(target, reason)
                onOperationError: message => root.terminalOperationError = message
                onTerminalClosed: root.terminalError = qsTr("The terminal closed.")
                onTerminalError: message => root.terminalError = message
            }
            Timer {
                interval: 300
                running: !terminal.terminalReady && root.terminalError.length === 0 && !root.away

                onRunningChanged: root.connectingLong = false
                onTriggered: root.connectingLong = true
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
                        visible: root.connectingLong
                        blinks: visible
                        color: KodosiTheme.terminalAccent
                    }
                    ShimmerText {
                        Layout.alignment: Qt.AlignHCenter
                        visible: root.connectingLong
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
                        objectName: root.objectName + ".away"
                        text: root.awayText
                        visible: root.away && root.terminalError.length === 0
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.terminalInkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("The terminal opens when its computer is back.")
                        visible: root.away && root.terminalError.length === 0
                        wrapMode: Text.WordWrap
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
        visible: !root.joined
        radius: KodosiTheme.radiusTile
        color: "transparent"
        border.width: 1
        border.color: root.ringed ? KodosiTheme.mix(KodosiTheme.terminalAccent, KodosiTheme.terminalHairline, 0.55) : KodosiTheme.alpha(KodosiTheme.terminalHairline, 0.8)

        Behavior on border.color { ColorAnimation { duration: KodosiTheme.motionFade } }
    }
    DosiTouch {
        anchors.fill: parent
        sessionId: root.sessionId
    }
    TerminalDropArea {
        anchors.fill: parent
        enabled: !root.roomEmbedded
        radius: KodosiTheme.radiusTile
        dashed: false
        objectName: root.objectName + ".drop"

        onTerminalDropped: id => {
            if (id === root.sessionId)
                return
            if (Models.DesktopState.stagedSessionIds.indexOf(id) < 0)
                Models.SessionActions.activate(id)
            Models.DesktopState.moveStaged(id, root.sessionId)
        }
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
            NumberAnimation { target: veil; property: "scale"; from: 0.04; to: 1; duration: KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
            NumberAnimation { target: veil; property: "opacity"; from: 0.6; to: 0; duration: KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
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
            fill: KodosiTheme.terminalLifted
            dark: true
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
            NumberAnimation { target: joinCard; property: "opacity"; to: 0; duration: KodosiTheme.reduceMotion ? 0 : 500 }
        }
    }
    KPopover {
        id: operationFailure
        x: Math.max(0, root.width - width - 8)
        y: 40
        transformOrigin: Popup.TopRight
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
    ClipboardReadDialog {
        id: clipboardRead
        objectName: root.objectName + ".clipboardRead"
        sessionName: root.sessionName

        onAnswered: allowed => terminal.answerClipboardRead(allowed)
    }
    TerminalLinkDialog {
        id: linkReview
        objectName: root.objectName + ".linkReview"

        onCopyRequested: target => terminal.copyText(target)
    }
    CloseTerminalDialog {
        id: closeConfirmation
        objectName: root.objectName + ".closeConfirmation"
    }
}
