import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property string sessionId: ""
    property string sessionName: ""

    function ask(id, name) {
        const session = Models.Sessions.presentationForSession(id);
        if (session.kind === "remote" && session.status === "offline") {
            offline.ask([id], session.hostLabel || "");
            return;
        }
        sessionId = id;
        sessionName = name;
        open();
    }
    function askOffline(ids, host) {
        offline.ask(ids, host);
    }

    destructive: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Close %1?").arg(sessionName)
    onOpened: standardButton(Dialog.Ok).text = qsTr("Close")
    onAccepted: Models.SessionActions.close(sessionId)

    PlainLabel {
        width: 340
        text: qsTr("Its programs stop.")
        color: KodosiTheme.inkMuted
        wrapMode: Text.WordWrap
    }
    OfflineTerminalDialog {
        id: offline
    }
}
