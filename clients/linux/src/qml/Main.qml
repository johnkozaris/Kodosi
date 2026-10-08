pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ApplicationWindow {
    id: window

    readonly property bool modalOpen: Models.DesktopState.modalOpen || Models.DesktopFiles.busy

    function openCreate(folder) {
        Models.SessionActions.create("", folder || Models.DesktopSettings.effectiveWorkingDirectory);
    }

    color: KodosiTheme.canvas
    height: 800
    minimumHeight: 560
    minimumWidth: 820
    objectName: "window.main"
    title: qsTr("Kodosi")
    visible: true
    width: 1240

    header: Models.AccessibilityScope {
        height: 54
        enabled: !window.modalOpen
        suppressed: window.modalOpen
        Rectangle { anchors.fill: parent; color: KodosiTheme.surface }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 12
            spacing: 8

            Image {
                Layout.rightMargin: 16
                Layout.preferredWidth: 26
                Layout.preferredHeight: 26
                source: "assets/kodosi-logo-dark.png"
                fillMode: Image.PreserveAspectFit
                Accessible.name: qsTr("Kodosi")
            }
            NavTab {
                accessibleId: "sessions"
                iconName: "terminal"
                selected: Models.DesktopState.activeView === Models.DesktopState.Sessions
                title: qsTr("Terminals")

                onClicked: Models.DesktopState.activeView = Models.DesktopState.Sessions
            }
            NavTab {
                accessibleId: "missions"
                iconName: "mission"
                selected: Models.DesktopState.activeView === Models.DesktopState.Missions
                title: qsTr("Rooms")

                onClicked: Models.DesktopState.activeView = Models.DesktopState.Missions
            }
            NavTab {
                accessibleId: "people"
                iconName: "people"
                selected: Models.DesktopState.activeView === Models.DesktopState.People
                title: qsTr("People")

                onClicked: Models.DesktopState.activeView = Models.DesktopState.People
            }
            Item {
                Layout.fillWidth: true
            }
            KButton {
                Accessible.id: objectName
                objectName: "header.sign-in"
                text: qsTr("Sign in")
                visible: !Models.Account.signedIn
                variant: KButton.Primary
                onClicked: {
                    Models.DesktopState.activeView = Models.DesktopState.Settings;
                    Models.Account.login();
                }
            }
            KButton {
                Accessible.id: objectName
                objectName: "header.approve-device"
                text: qsTr("Approve this device")
                visible: Models.Account.signedIn && !Models.Devices.localDeviceEnrolled
                variant: KButton.Primary
                onClicked: {
                    settings.section = 1;
                    Models.DesktopState.activeView = Models.DesktopState.Settings;
                }
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Settings")
                objectName: "header.settings"
                glyph: "settings"
                onClicked: Models.DesktopState.activeView = Models.DesktopState.Settings
            }
        }
    }

    onClosing: close => {
        close.accepted = false;
        window.hide();
    }

    Models.AccessibilityScope {
        anchors.fill: parent
        suppressed: window.modalOpen

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: errorRow.implicitHeight + 16
                color: KodosiTheme.surfaceRaised
                visible: Models.AppState.error.length > 0 || Models.DesktopState.lastError.length > 0 || Models.DesktopFiles.errorMessage.length > 0

                RowLayout {
                    id: errorRow

                    anchors.fill: parent
                    anchors.margins: 8

                    KButton {
                        Layout.fillWidth: true
                        text: qsTr("Action failed")
                        iconName: "warning"
                        variant: KButton.Quiet
                        onClicked: errorDetails.open()
                    }
                    KIconButton {
                        Accessible.id: objectName
                        Accessible.name: qsTr("Dismiss error")
                        glyph: "close"
                        objectName: "window.error.dismiss"

                        onClicked: {
                            Models.AppState.clearError();
                            Models.DesktopState.clearError();
                            Models.DesktopFiles.clearError();
                        }
                    }
                }
            }
            StackLayout {
                Layout.fillHeight: true
                Layout.fillWidth: true
                currentIndex: Models.DesktopState.activeView

                RowLayout {
                    spacing: 0

                    SessionSidebar {
                        Layout.fillHeight: true
                        Layout.preferredWidth: 230
                        visible: Models.DesktopState.sidebarOpen

                        onDetailsRequested: sessionId => details.openSession(sessionId)
                        onNewSessionRequested: folder => window.openCreate(folder)
                        onHistoryRequested: history.openModal()
                    }
                    Loader {
                        id: stageLoader
                        Layout.fillHeight: true
                        Layout.fillWidth: true
                        active: Models.DesktopState.activeView === Models.DesktopState.Sessions
                        sourceComponent: TerminalStage {
                            interactionEnabled: !window.modalOpen
                            sidebarOpen: Models.DesktopState.sidebarOpen
                            onInspectSessionRequested: (id, name) => details.openSession(id)
                            onNewSessionRequested: window.openCreate("")
                            onShareSessionRequested: (id, name) => details.openSession(id)
                            onShowSidebarRequested: Models.DesktopState.sidebarOpen = true
                        }
                    }
                }
                MissionsView { onInspectSessionRequested: id => details.openSession(id) }
                PeopleView {
                }
                SettingsView {
                    id: settings
                }
            }
        }
    }
    Connections {
        function onDirectoryPicked(purpose, path) {
            if (purpose === "new") {
                Models.DesktopSettings.setWorkingDirectory(path);
                window.openCreate(path);
            }
        }

        target: Models.DesktopFiles
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
    KPopover {
        id: errorDetails
        parent: Overlay.overlay
        x: (parent.width - width) / 2
        y: 80
        width: 400
        padding: 18
        contentItem: KReadOnlyText {
            text: Models.AppState.error || Models.DesktopState.lastError || Models.DesktopFiles.errorMessage
        }
    }
    SessionDetails {
        id: details
    }
    HistoryView {
        id: history
    }
    StartupOverlay {
        id: startup
    }
    Shortcut {
        enabled: !window.modalOpen
        sequence: "Ctrl+Shift+N"

        onActivated: window.openCreate()
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
