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
    property bool componentReady: false
    property bool focusedSizeAuthority: false
    required property string sessionId
    property string sessionName: ""
    property string headerTitle: ""
    property string program: ""
    property bool shareable: false
    property string notice: ""
    property bool reconnecting: false
    property string terminalError: ""
    property string terminalOperationError: ""

    signal inspectSessionRequested(string sessionId, string sessionName)
    signal shareSessionRequested(string sessionId, string sessionName)

    function bindTerminal() {
        terminalError = "";
        terminalOperationError = "";
        if (!Models.TerminalSurfaces.bind(terminal, sessionId))
            terminalError = qsTr("This terminal is no longer available.");
    }
    function forceTerminalFocus() {
        if (visible && enabled)
            terminal.forceActiveFocus(Qt.ShortcutFocusReason);
    }
    function refreshSession() {
        const session = Models.Sessions.presentationForSession(sessionId);
        sessionName = session.name || qsTr("Terminal");
        headerTitle = session.headerTitle || sessionName;
        program = session.program || "";
        shareable = session.kind === "local" && session.isOwner === true;
        reconnecting = session.kind === "remote" && session.connectionState === "connected" && session.status === "reconnecting";
        notice = session.kind === "remote" && session.connectionState === "connected" ? session.message || "" : "";
    }

    Accessible.id: objectName
    Accessible.ignored: accessibilitySuppressed || !visible
    Accessible.name: qsTr("%1 terminal").arg(sessionName)
    Accessible.role: Accessible.Pane
    objectName: "stage.tile." + sessionId

    Component.onCompleted: {
        componentReady = true;
        refreshSession();
        bindTerminal();
    }
    Component.onDestruction: Models.TerminalSurfaces.detach(terminal)
    onSessionIdChanged: {
        if (componentReady) {
            Models.TerminalSurfaces.detach(terminal);
            refreshSession();
            bindTerminal();
        }
    }

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
    Rectangle {
        anchors.fill: parent
        color: KodosiTheme.terminal
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 34
            color: root.active ? KodosiTheme.surfaceRaised : KodosiTheme.surface

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 6
                spacing: 6

                ProviderIcon { program: root.program; Layout.preferredWidth: 18; Layout.preferredHeight: 18 }
                KComboBox {
                    Layout.fillWidth: true
                    visible: root.roomEmbedded
                    model: Models.Missions.sessionIds.map(id => ({ id: id, name: Models.Sessions.presentationForSession(id).headerTitle }))
                    textRole: "name"
                    valueRole: "id"
                    currentIndex: Models.Missions.sessionIds.indexOf(root.sessionId)
                    Accessible.name: qsTr("Switch terminal")
                    objectName: "room.terminal.switcher"
                    onActivated: Models.SessionActions.activateInRoom(currentValue)
                }
                PlainLabel {
                    Layout.fillWidth: true
                    color: KodosiTheme.textPrimary
                    elide: Text.ElideRight
                    font.pixelSize: 12
                    text: root.headerTitle
                    visible: !root.roomEmbedded
                }
                PlainLabel {
                    Accessible.id: objectName
                    color: KodosiTheme.textSecondary
                    font.pixelSize: 11
                    objectName: root.objectName + ".reconnecting"
                    text: qsTr("Reconnecting")
                    visible: root.reconnecting
                }
                PlainLabel {
                    Accessible.id: objectName
                    color: KodosiTheme.textSecondary
                    font.pixelSize: 11
                    objectName: root.objectName + ".notice"
                    text: root.notice
                    visible: root.notice.length > 0
                }
                KIconButton {
                    glyph: "warning"
                    visible: root.terminalOperationError.length > 0
                    Accessible.name: qsTr("Terminal action failed")
                    onClicked: operationFailure.open()
                }
                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: qsTr("Terminal details")
                    glyph: "document"
                    objectName: root.objectName + ".details"

                    onClicked: root.inspectSessionRequested(root.sessionId, root.sessionName)
                }
                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: root.focusedSizeAuthority ? qsTr("Return to tiles") : qsTr("Maximize")
                    glyph: "focus"
                    objectName: root.objectName + ".focus"
                    visible: !root.roomEmbedded

                    onClicked: {
                        Models.DesktopState.selectSession(root.sessionId);
                        Models.DesktopState.toggleFocusForSelectedSession();
                    }
                }
                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: qsTr("Share terminal")
                    glyph: "people"
                    objectName: root.objectName + ".share"
                    visible: root.shareable
                    onClicked: root.shareSessionRequested(root.sessionId, root.sessionName)
                }
                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: qsTr("Minimize")
                    glyph: "minus"
                    objectName: root.objectName + ".minimize"
                    onClicked: Models.SessionActions.minimize(root.sessionId)
                }
                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: qsTr("Close terminal")
                    glyph: "close"
                    objectName: root.objectName + ".close"
                    onClicked: closeConfirmation.open()
                }
            }
        }
        Item {
            Layout.fillHeight: true
            Layout.fillWidth: true

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
                preeditBackground: KodosiTheme.accent
                preeditForeground: KodosiTheme.accentForeground
                scrollbackLines: Models.DesktopSettings.scrollbackLines
                selectionBackground: KodosiTheme.accent
                selectionForeground: KodosiTheme.accentForeground

                onActiveFocusChanged: {
                    if (activeFocus)
                        Models.DesktopState.selectSession(root.sessionId);
                }
                onContextMenuRequested: (x, y) => {
                    const point = terminal.mapToItem(root, x, y);
                    context.popup(point.x, point.y);
                }
                onOperationError: message => root.terminalOperationError = message
                onTerminalClosed: root.terminalError = qsTr("The terminal has ended.")
                onTerminalError: message => root.terminalError = message
            }
            Rectangle {
                anchors.fill: parent
                color: KodosiTheme.surface
                visible: !terminal.terminalReady || root.terminalError.length > 0

                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 12
                    width: Math.min(320, parent.width - 32)

                    KBusyIndicator {
                        Layout.alignment: Qt.AlignHCenter
                        running: root.terminalError.length === 0
                        visible: running
                        Accessible.name: qsTr("Connecting terminal")
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.textPrimary
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("Terminal unavailable")
                        visible: root.terminalError.length > 0
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        color: KodosiTheme.textSecondary
                        horizontalAlignment: Text.AlignHCenter
                        text: root.terminalError
                        visible: root.terminalError.length > 0
                        wrapMode: Text.WordWrap
                    }
                    KButton {
                        Accessible.id: objectName
                        Layout.alignment: Qt.AlignHCenter
                        objectName: root.objectName + ".retry"
                        text: qsTr("Retry")
                        visible: root.terminalError.length > 0

                        onClicked: {
                            root.terminalError = "";
                            const activated = root.roomEmbedded ? Models.SessionActions.activateInRoom(root.sessionId) : Models.SessionActions.activate(root.sessionId);
                            if (activated && !Models.TerminalSurfaces.retry(terminal, root.sessionId))
                                root.terminalError = qsTr("The terminal could not reconnect.");
                        }
                    }
                }
            }
        }
    }
    KPopover {
        id: operationFailure
        x: Math.max(0, root.width - width - 8)
        y: 34
        width: Math.min(360, root.width - 16)
        padding: 16
        contentItem: ColumnLayout {
            spacing: 12
            KReadOnlyText { Layout.fillWidth: true; text: root.terminalOperationError; objectName: root.objectName + ".operationError" }
            KButton {
                text: qsTr("Dismiss")
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
            objectName: "panel.terminalTile.copy"
            text: qsTr("Copy")

            onTriggered: terminal.copySelectionToClipboard()
        }
        KMenuItem {
            Accessible.id: objectName
            enabled: terminal.terminalReady
            objectName: "panel.terminalTile.paste"
            text: qsTr("Paste")

            onTriggered: terminal.pasteFromClipboard()
        }
    }
    KDialog {
        id: closeConfirmation

        objectName: root.objectName + ".closeConfirmation"
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Close %1?").arg(root.sessionName)
        onOpened: standardButton(Dialog.Ok).text = qsTr("Close")

        onAccepted: Models.SessionActions.close(root.sessionId)

        PlainLabel {
            color: KodosiTheme.textPrimary
            width: 360
            wrapMode: Text.WordWrap
            text: qsTr("Running programs will stop.")
        }
    }
}
