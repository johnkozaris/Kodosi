import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    signal resumeRequested

    function openFor(folder, roomId) {
        if (folder)
            Models.DesktopState.launcherFolder = folder
        launcher.roomId = roomId || ""
        open()
    }

    focus: true
    modal: true
    objectName: "launcher.sheet"
    padding: 0
    parent: Overlay.overlay
    radius: KodosiTheme.radiusSheet
    elevation: 3
    fill: KodosiTheme.surface
    width: Math.min(608, parent ? parent.width - 32 : 608)
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0

    contentItem: ColumnLayout {
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 60
            Layout.leftMargin: 24
            Layout.rightMargin: 14

            PlainLabel {
                Layout.fillWidth: true
                font.pixelSize: KodosiTheme.fontTitle
                font.weight: Font.DemiBold
                text: qsTr("New terminal")
            }
            KIconButton {
                Accessible.id: objectName
                Accessible.name: qsTr("Close")
                glyph: "close"
                objectName: "launcher.close"

                onClicked: root.close()
            }
        }
        AgentLauncher {
            id: launcher

            Layout.fillWidth: true
            Layout.leftMargin: 24
            Layout.rightMargin: 24
            Layout.bottomMargin: 24
            inSheet: true
            onStarted: root.close()
            onResumeRequested: {
                root.close()
                root.resumeRequested()
            }
        }
    }
}
