import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property bool restart: false
    property int terminals: 0

    function request(restarting) {
        restart = restarting;
        terminals = Models.Sessions.sessions.filter(session => session.kind === "local").length;
        if (terminals === 0 || Models.ApplicationLifecycle.hostForeground)
            finish();
        else
            open();
    }
    function finish() {
        if (restart)
            Models.ApplicationLifecycle.restart();
        else
            Qt.quit();
    }

    destructive: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    title: restart ? qsTr("Restart Kodosi?") : qsTr("Quit Kodosi?")

    onAccepted: finish()
    onOpened: standardButton(Dialog.Ok).text = restart ? qsTr("Restart") : qsTr("Quit")

    PlainLabel {
        color: KodosiTheme.inkMuted
        text: root.terminals === 1 ? qsTr("1 terminal on this computer stops.")
            : qsTr("%1 terminals on this computer stop.").arg(root.terminals)
        width: 340
        wrapMode: Text.WordWrap
    }
}
