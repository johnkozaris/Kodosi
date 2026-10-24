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
    readonly property bool updateNotice: Models.ApplicationLifecycle.updateInstalled && !updateNoticeDismissed
    property bool updateNoticeDismissed: false
    // Windows installs the update only when no program of Kodosi runs, and `kodosi host` in a terminal is one.
    readonly property string updateText: Qt.platform.os === "windows" && Models.ApplicationLifecycle.hostForeground
        ? qsTr("Kodosi was updated. Stop the kodosi host command, then restart Kodosi.")
        : qsTr("Kodosi was updated. Restart it to use the new version.")
    readonly property bool drawsTitleBar: Qt.platform.os === "windows"
    readonly property bool accountReady: Models.Account.signedIn && Models.Devices.localDeviceEnrolled
    readonly property string deviceNotice: {
        if (!Models.Account.signedIn
            || (Models.DesktopState.activeView === Models.DesktopState.Settings && (settingsLoader.item as SettingsView)?.section === 3))
            return "";
        if (accountReady) {
            const requests = Models.Devices.waitingRequests;
            if (requests.length === 1)
                return qsTr("%1 wants to use your account.").arg(requests[0].deviceLabel);
            return requests.length > 1 ? qsTr("%1 devices want to use your account.").arg(requests.length) : "";
        }
        if (Models.Devices.approvalNoticeDismissed)
            return "";
        return Models.Devices.approvalCode.length > 0
            ? qsTr("This computer waits for approval from another of your devices.")
            : qsTr("This computer needs approval before it can share.");
    }

    function openLauncher(roomId) {
        const stage = stageLoader.item as TerminalStage;
        if (!roomId && stage && stage.focusLauncher())
            return;
        window.launcherSheet().openFor(roomId || "");
    }
    function sessionDetails() {
        detailsLoader.active = true;
        return detailsLoader.item as SessionDetails;
    }
    function shareSheet() {
        shareLoader.active = true;
        return shareLoader.item as ShareSheet;
    }
    function historyView() {
        historyLoader.active = true;
        return historyLoader.item as HistoryView;
    }
    function launcherSheet() {
        launcherLoader.active = true;
        return launcherLoader.item as LauncherSheet;
    }
    function commandPalette() {
        paletteLoader.active = true;
        return paletteLoader.item as CommandPalette;
    }
    function aboutDialog() {
        aboutLoader.active = true;
        return aboutLoader.item as AboutDialog;
    }
    function restartDialog() {
        restartLoader.active = true;
        return restartLoader.item as RestartDialog;
    }
    function missionsView() {
        missionsLoader.active = true;
        return missionsLoader.item as MissionsView;
    }
    function settingsView() {
        settingsLoader.active = true;
        return settingsLoader.item as SettingsView;
    }
    function openNewRoom() {
        Models.DesktopState.activeView = Models.DesktopState.Missions;
        window.missionsView().openCreate();
    }
    function openAdjacentTerminal(offset) {
        const staged = Models.DesktopState.stagedSessionIds;
        if (staged.length === 0)
            return;
        const index = Math.max(0, staged.indexOf(Models.DesktopState.selectedSessionId));
        Models.SessionActions.activate(staged[(index + offset + staged.length) % staged.length]);
    }

    color: KodosiTheme.ground
    flags: drawsTitleBar
        ? Qt.Window | Qt.ExpandedClientAreaHint | Qt.NoTitleBarBackgroundHint | Qt.CustomizeWindowHint
        : Qt.Window
    height: 820
    objectName: "window.main"
    title: qsTr("Kodosi")
    topPadding: 0
    visible: true
    width: 1280

    onClosing: close => {
        close.accepted = false;
        if (Models.DesktopSettings.keepRunning) {
            window.hide();
            Models.Tray.noteHidden();
        } else {
            Qt.quit();
        }
    }

    Connections {
        function onQuitRequested() {
            Qt.quit();
        }

        target: Models.Tray
    }
    Connections {
        function onRestartOffered() {
            window.restartDialog().request();
        }

        target: Models.ApplicationLifecycle
    }
    Loader {
        id: restartLoader
        active: false
        sourceComponent: RestartDialog {
            objectName: "window.restart"

            onAboutToShow: {
                window.show();
                window.raise();
                window.requestActivate();
            }
        }
    }

    Binding {
        target: KodosiTheme
        property: "shown"
        value: window.visible && window.visibility !== Window.Minimized
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

                onDetailsRequested: sessionId => window.sessionDetails().openSession(sessionId)
                onShareRequested: sessionId => window.shareSheet().openSession(sessionId)
                onNewTerminalRequested: window.openLauncher()
                onResumeRequested: provider => window.historyView().openModal(provider)
                onPaletteRequested: window.commandPalette().open()
                onNewRoomRequested: window.openNewRoom()
                onDosiSettingsRequested: {
                    Models.DesktopState.activeView = Models.DesktopState.Settings
                    window.settingsView().section = 4
                }
                onAccountSettingsRequested: {
                    Models.DesktopState.activeView = Models.DesktopState.Settings
                    if (Models.Devices.requests.length > 0)
                        window.settingsView().showDevices(true)
                    else
                        window.settingsView().section = 2
                }
                onAboutRequested: window.aboutDialog().open()
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
                            onInspectSessionRequested: id => window.sessionDetails().openSession(id)
                            onResumeRequested: window.historyView().openModal()
                            onShareSessionRequested: id => window.shareSheet().openSession(id)
                        }
                    }
                    Loader {
                        id: missionsLoader
                        readonly property bool current: Models.DesktopState.activeView === Models.DesktopState.Missions
                        active: false
                        onCurrentChanged: if (current) active = true
                        Component.onCompleted: if (current) active = true
                        sourceComponent: MissionsView {
                            onNewTerminalRequested: roomId => window.openLauncher(roomId)
                            onInspectSessionRequested: id => window.sessionDetails().openSession(id)
                            onShareSessionRequested: id => window.shareSheet().openSession(id)
                        }
                    }
                    Loader {
                        readonly property bool current: Models.DesktopState.activeView === Models.DesktopState.People
                        active: false
                        onCurrentChanged: if (current) active = true
                        Component.onCompleted: if (current) active = true
                        sourceComponent: PeopleView {}
                    }
                    Loader {
                        id: settingsLoader
                        readonly property bool current: Models.DesktopState.activeView === Models.DesktopState.Settings
                        active: false
                        onCurrentChanged: if (current) active = true
                        Component.onCompleted: if (current) active = true
                        sourceComponent: SettingsView {}
                    }
                }
                NoticePill {
                    id: appNotice
                    readonly property bool shown: window.updateNotice || window.notice.length > 0

                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: shown ? 10 : -height
                    width: Math.min(implicitWidth, parent.width - 80)
                    height: implicitHeight
                    opacity: shown ? 1 : 0
                    visible: opacity > 0
                    identifier: "window.error"
                    text: window.updateNotice ? window.updateText : window.notice
                    actionText: window.updateNotice ? qsTr("Restart") : ""

                    Behavior on anchors.topMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                    onActionClicked: window.restartDialog().request()
                    onDismissed: {
                        if (window.updateNotice) {
                            window.updateNoticeDismissed = true;
                            return;
                        }
                        Models.AppState.clearError();
                        Models.DesktopState.clearError();
                        Models.DesktopFiles.clearError();
                    }
                }
                NoticePill {
                    readonly property bool shown: window.deviceNotice.length > 0 && !appNotice.shown

                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: shown ? 10 : -height
                    width: Math.min(implicitWidth, parent.width - 80)
                    height: implicitHeight
                    opacity: shown ? 1 : 0
                    visible: opacity > 0
                    identifier: "window.devices"
                    sign: ({ sign: "hand", asks: true, signLabel: qsTr("Needs your approval") })
                    text: window.deviceNotice
                    actionText: window.accountReady ? qsTr("Approve…")
                        : Models.Devices.approvalCode.length > 0 ? qsTr("Show code") : qsTr("Open Settings")

                    Behavior on anchors.topMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                    onActionClicked: {
                        Models.DesktopState.activeView = Models.DesktopState.Settings;
                        window.settingsView().showDevices(window.accountReady);
                    }
                    onDismissed: {
                        if (window.accountReady)
                            Models.Devices.dismissRequests();
                        else
                            Models.Devices.dismissApprovalNotice();
                    }
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
    }
    Component {
        id: tipBackground
        Raised { radius: KodosiTheme.radiusSm; fill: KodosiTheme.lifted; elevation: 2 }
    }
    Component {
        id: tipContent
        PlainLabel { font.pixelSize: KodosiTheme.fontCaption; font.weight: Font.Medium }
    }
    Component {
        id: tipEnter
        Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Tip.warm ? 0 : KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
            NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: Tip.warm ? 0 : KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        }
    }
    Timer {
        id: tipCool
        interval: 600
        onTriggered: Tip.warm = false
    }
    Connections {
        function onOpened() {
            tipCool.stop();
            Tip.warm = true;
        }
        function onClosed() {
            tipCool.restart();
        }

        target: glow.ToolTip.toolTip
    }
    Component.onCompleted: {
        const tip = glow.ToolTip.toolTip;
        tip.padding = 7;
        const surface = tipBackground.createObject(glow) as Raised;
        const label = tipContent.createObject(glow) as PlainLabel;
        surface.parent = null;
        label.parent = null;
        label.text = Qt.binding(() => tip.text);
        tip.background = surface;
        tip.contentItem = label;
        tip.enter = tipEnter.createObject(glow);
        tip.exit = null;
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
    Loader {
        id: detailsLoader
        active: false
        sourceComponent: SessionDetails {
            onShareRequested: sessionId => window.shareSheet().openSession(sessionId)
        }
    }
    Loader {
        id: shareLoader
        active: false
        sourceComponent: ShareSheet {}
    }
    Loader {
        id: historyLoader
        active: false
        sourceComponent: HistoryView {}
    }
    Loader {
        id: launcherLoader
        active: false
        sourceComponent: LauncherSheet {
            onResumeRequested: window.historyView().openModal()
        }
    }
    Loader {
        id: paletteLoader
        active: false
        sourceComponent: CommandPalette {
            onNewTerminalRequested: window.openLauncher()
            onNewRoomRequested: window.openNewRoom()
            onResumeRequested: window.historyView().openModal()
        }
    }
    Loader {
        id: aboutLoader
        active: false
        sourceComponent: AboutDialog {}
    }
    StartupOverlay {}
    Shortcut {
        enabled: !window.modalOpen
        sequence: "Ctrl+Shift+N"

        onActivated: window.openLauncher()
    }
    Shortcut {
        enabled: !window.modalOpen
        sequences: ["Ctrl+Shift+K", "Ctrl+K"]

        onActivated: window.commandPalette().open()
    }
    Shortcut {
        enabled: !window.modalOpen
        sequence: "Ctrl+Shift+R"

        onActivated: window.historyView().openModal()
    }
    Shortcut {
        enabled: !window.modalOpen
        sequences: ["Ctrl+Shift+,", "Ctrl+,"]

        onActivated: Models.DesktopState.activeView = Models.DesktopState.Settings
    }
    Shortcut {
        enabled: !window.modalOpen && Models.DesktopState.activeView === Models.DesktopState.Sessions
            && Models.DesktopState.selectedSessionId.length > 0
        sequence: "Ctrl+Shift+I"

        onActivated: window.sessionDetails().openSession(Models.DesktopState.selectedSessionId)
    }
    Shortcut {
        enabled: !window.modalOpen && Models.DesktopState.activeView === Models.DesktopState.Sessions
        sequences: ["Ctrl+Shift+[", "Ctrl+{"]

        onActivated: window.openAdjacentTerminal(-1)
    }
    Shortcut {
        enabled: !window.modalOpen && Models.DesktopState.activeView === Models.DesktopState.Sessions
        sequences: ["Ctrl+Shift+]", "Ctrl+}"]

        onActivated: window.openAdjacentTerminal(1)
    }
    Shortcut {
        enabled: !window.modalOpen && Models.DesktopState.dosiOn
        sequence: "Ctrl+Shift+J"

        onActivated: Models.DesktopState.toggleDosi()
    }
    Shortcut {
        enabled: !window.modalOpen && Models.Dosi.busy && Models.DesktopState.dosiOpen && Models.DesktopState.sidebarOpen
        sequence: "Ctrl+Shift+."

        onActivated: Models.Dosi.stop()
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
