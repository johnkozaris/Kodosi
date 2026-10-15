pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import Kodosi.Models 1.0 as Models

ApplicationWindow {
    id: window

    readonly property bool modalOpen: Models.DesktopState.modalOpen || Models.DesktopFiles.busy
    readonly property string notice: Models.AppState.error || Models.DesktopState.lastError || Models.DesktopFiles.errorMessage
    readonly property bool drawsTitleBar: Qt.platform.os === "windows"

    function openLauncher(folder, roomId) {
        const stage = stageLoader.item as TerminalStage;
        if (!roomId && stage && stage.focusLauncher(folder || ""))
            return;
        launcherSheet.openFor(folder || "", roomId || "");
    }
    function openNewRoom() {
        Models.DesktopState.activeView = Models.DesktopState.Missions;
        missions.openCreate();
    }

    color: KodosiTheme.ground
    flags: drawsTitleBar
        ? Qt.Window | Qt.ExpandedClientAreaHint | Qt.NoTitleBarBackgroundHint | Qt.CustomizeWindowHint
        : Qt.Window
    height: 820
    minimumHeight: 560
    minimumWidth: 860
    objectName: "window.main"
    title: qsTr("Kodosi")
    topPadding: 0
    visible: true
    width: 1280

    onClosing: close => {
        close.accepted = false;
        window.hide();
    }

    Models.AccessibilityScope {
        anchors.fill: parent
        enabled: !window.modalOpen
        suppressed: window.modalOpen

        Shape {
            id: glow
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                strokeColor: "transparent"
                strokeWidth: 0
                fillGradient: RadialGradient {
                    centerX: 70
                    centerY: 10
                    focalX: centerX
                    focalY: centerY
                    centerRadius: 560
                    GradientStop { position: 0; color: KodosiTheme.alpha(KodosiTheme.accent, KodosiTheme.isDark ? 0.11 : 0.14) }
                    GradientStop { position: 1; color: KodosiTheme.alpha(KodosiTheme.accent, 0) }
                }
                PathRectangle { width: glow.width; height: glow.height }
            }
        }
        RowLayout {
            anchors.fill: parent
            spacing: 0

            Sidebar {
                Layout.fillHeight: true
                Layout.preferredWidth: implicitWidth
                titleBar: window.drawsTitleBar

                onDetailsRequested: sessionId => details.openSession(sessionId)
                onShareRequested: sessionId => share.openSession(sessionId)
                onNewTerminalRequested: folder => window.openLauncher(folder)
                onResumeRequested: history.openModal()
                onPaletteRequested: palette.open()
                onNewRoomRequested: window.openNewRoom()
            }
            Item {
                Layout.fillHeight: true
                Layout.fillWidth: true
                Layout.topMargin: window.drawsTitleBar ? KodosiTheme.titleBarHeight : KodosiTheme.frameInset
                Layout.bottomMargin: KodosiTheme.frameInset
                Layout.rightMargin: KodosiTheme.frameInset

                Raised {
                    anchors.fill: parent
                    radius: KodosiTheme.radiusXl
                    fill: KodosiTheme.surface
                }
                StackLayout {
                    anchors.fill: parent
                    anchors.margins: 1
                    currentIndex: Models.DesktopState.activeView

                    Loader {
                        id: stageLoader
                        active: Models.DesktopState.activeView === Models.DesktopState.Sessions
                        sourceComponent: TerminalStage {
                            interactionEnabled: !window.modalOpen
                            onInspectSessionRequested: id => details.openSession(id)
                            onResumeRequested: history.openModal()
                            onShareSessionRequested: id => share.openSession(id)
                        }
                    }
                    MissionsView {
                        id: missions
                        onNewTerminalRequested: roomId => window.openLauncher("", roomId)
                        onInspectSessionRequested: id => details.openSession(id)
                        onShareSessionRequested: id => share.openSession(id)
                    }
                    PeopleView {}
                    SettingsView {}
                }
            }
        }
        WindowControls {
            visible: window.drawsTitleBar
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.topMargin: (KodosiTheme.titleBarHeight - height) / 2
            anchors.rightMargin: KodosiTheme.frameInset
        }
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: window.notice.length > 0 ? 22 : -height
            width: Math.min(noticeRow.implicitWidth + 28, parent.width - 80)
            height: Math.max(40, noticeText.implicitHeight + 20)
            opacity: window.notice.length > 0 ? 1 : 0
            visible: opacity > 0

            Behavior on anchors.bottomMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

            Raised {
                anchors.fill: parent
                radius: Math.min(20, height / 2)
                fill: KodosiTheme.lifted
                elevation: 3
            }
            RowLayout {
                id: noticeRow
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 8
                spacing: 10

                Rectangle { Layout.preferredWidth: 7; Layout.preferredHeight: 7; radius: 3.5; color: KodosiTheme.caution }
                PlainLabel {
                    id: noticeText
                    Layout.fillWidth: true
                    Layout.maximumWidth: 520
                    text: window.notice
                    wrapMode: Text.WordWrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    objectName: "window.error.text"
                    Accessible.id: objectName
                }
                KIconButton {
                    Accessible.id: objectName
                    Accessible.name: qsTr("Dismiss")
                    glyph: "close"
                    size: 26
                    objectName: "window.error.dismiss"

                    onClicked: {
                        Models.AppState.clearError();
                        Models.DesktopState.clearError();
                        Models.DesktopFiles.clearError();
                    }
                }
            }
        }
    }
    Component {
        id: tipBackground
        Raised { radius: KodosiTheme.radiusSm; fill: KodosiTheme.lifted; elevation: 2 }
    }
    Component {
        id: tipContent
        PlainLabel { font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
    }
    Component.onCompleted: {
        const tip = glow.ToolTip.toolTip;
        tip.padding = 7;
        const surface = tipBackground.createObject(glow);
        const label = tipContent.createObject(glow);
        surface.parent = null;
        label.parent = null;
        label.text = Qt.binding(() => tip.text);
        tip.background = surface;
        tip.contentItem = label;
    }
    Connections {
        function onSignInPageReady(page) {
            Models.DesktopFiles.openWebUrl(page);
        }
        function onBrowserAnswered() {
            window.show();
            window.raise();
            window.requestActivate();
        }

        target: Models.Account
    }
    Connections {
        function onActivated(sessionId) {
            window.show();
            window.raise();
            const currentStage = stageLoader.item as TerminalStage;
            if (currentStage) currentStage.scheduleTerminalFocus();
        }

        target: Models.SessionActions
    }
    SessionDetails {
        id: details
        onShareRequested: sessionId => share.openSession(sessionId)
    }
    ShareSheet {
        id: share
    }
    HistoryView {
        id: history
    }
    LauncherSheet {
        id: launcherSheet
        onResumeRequested: history.openModal()
    }
    CommandPalette {
        id: palette
        onNewTerminalRequested: folder => window.openLauncher(folder)
        onNewRoomRequested: window.openNewRoom()
        onResumeRequested: history.openModal()
    }
    StartupOverlay {}
    Shortcut {
        enabled: !window.modalOpen
        sequence: "Ctrl+Shift+N"

        onActivated: window.openLauncher()
    }
    Shortcut {
        enabled: !window.modalOpen
        sequence: "Ctrl+K"

        onActivated: palette.open()
    }
    Shortcut {
        enabled: !window.modalOpen
        sequence: "Ctrl+Shift+B"

        onActivated: Models.DesktopState.sidebarOpen = !Models.DesktopState.sidebarOpen
    }
    Shortcut {
        enabled: !window.modalOpen && Models.DesktopState.activeView === Models.DesktopState.Sessions
        sequence: "Ctrl+Shift+F"

        onActivated: Models.DesktopState.toggleFocusForSelectedSession()
    }
    Shortcut {
        enabled: !window.modalOpen && Models.DesktopState.activeView === Models.DesktopState.Sessions
        sequence: "Ctrl+Shift+W"

        onActivated: Models.SessionActions.minimize(Models.DesktopState.selectedSessionId)
    }
}
