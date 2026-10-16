import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property bool loading: false
    property bool failed: false

    signal resumeRequested

    function focusLauncher(folder) {
        if (!launcher.visible)
            return false
        launcher.focusPrompt(folder)
        return true
    }

    objectName: "stage.overview"
    Accessible.id: objectName

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(420, parent.width - 48)
        visible: root.failed
        title: qsTr("The terminals did not load")
        message: Models.Sessions.authorityError
        art: Well {
            width: 52
            height: 52
            KIcon { anchors.centerIn: parent; width: 22; height: 22; name: "warning"; color: KodosiTheme.caution }
        }

        KButton {
            objectName: "stage.sessions.error.retry"
            Accessible.id: objectName
            variant: KButton.Primary
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: Models.SessionActions.refresh()
        }
    }
    ColumnLayout {
        anchors.centerIn: parent
        visible: root.loading && !root.failed
        spacing: 14

        CursorBlock { Layout.alignment: Qt.AlignHCenter; Layout.preferredWidth: 12; Layout.preferredHeight: 22; blinks: visible }
        ShimmerText {
            Layout.alignment: Qt.AlignHCenter
            objectName: "stage.sessions.loading"
            text: qsTr("Loading")
            active: visible
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
        }
    }
    AgentLauncher {
        id: launcher
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(40, parent.height / 2 - 150)
        width: Math.min(560, parent.width - 48)
        visible: !root.loading && !root.failed
        centered: true
        onResumeRequested: root.resumeRequested()
    }
}
