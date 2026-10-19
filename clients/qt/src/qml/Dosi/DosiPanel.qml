pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    signal newTerminalRequested
    signal newRoomRequested
    signal settingsRequested

    objectName: "dosi.panel"
    Accessible.role: Accessible.Pane
    Accessible.name: "Dosi"
    onVisibleChanged: {
        if (!visible)
            return
        arrive.play()
        Qt.callLater(composer.focusField)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        DosiShelf {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            Layout.topMargin: 10
            onNewTerminalRequested: root.newTerminalRequested()
            onNewRoomRequested: root.newRoomRequested()
        }
        DosiThread {
            Layout.fillWidth: true
            Layout.fillHeight: true
            listening: composer.focused
            opacity: arrive.progress
            transform: Arrive { id: arrive; delay: 120; rise: 8 }
            onSettingsRequested: root.settingsRequested()
        }
        DosiComposer {
            id: composer
            Layout.fillWidth: true
        }
    }
}
