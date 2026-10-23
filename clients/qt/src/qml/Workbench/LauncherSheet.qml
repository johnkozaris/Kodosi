import Kodosi 1.0
import QtQuick
import QtQuick.Controls
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
    padding: 24
    parent: Overlay.overlay
    radius: KodosiTheme.radiusSheet
    elevation: 3
    fill: KodosiTheme.surface
    width: Math.min(608, parent ? parent.width - 32 : 608)
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0

    contentItem: AgentLauncher {
        id: launcher

        inSheet: true
        onCloseRequested: root.close()
        onStarted: root.close()
        onResumeRequested: {
            root.close()
            root.resumeRequested()
        }
    }
}
