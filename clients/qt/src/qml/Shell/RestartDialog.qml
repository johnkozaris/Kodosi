import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property int terminals: 0

    function request() {
        terminals = Models.Sessions.sessions.filter(session => session.kind === "local").length;
        if (terminals === 0 || Models.ApplicationLifecycle.hostForeground)
            Models.ApplicationLifecycle.restart();
        else
            open();
    }

    destructive: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Restart Kodosi to update?")

    onAccepted: Models.ApplicationLifecycle.restart()
    onOpened: standardButton(Dialog.Ok).text = qsTr("Restart")

    PlainLabel {
        color: KodosiTheme.inkMuted
        text: root.terminals === 1 ? qsTr("1 terminal on this computer stops.")
            : qsTr("%1 terminals on this computer stop.").arg(root.terminals)
        width: 340
        wrapMode: Text.WordWrap
    }
}
