import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property var sessionIds: []
    property string host: ""

    function ask(ids, hostName) {
        sessionIds = ids;
        host = hostName;
        open();
    }

    standardButtons: Dialog.Ok | Dialog.Discard | Dialog.Cancel
    title: qsTr("%1 is offline").arg(host)
    onOpened: {
        standardButton(Dialog.Ok).text = qsTr("Hide until it's back");
        standardButton(Dialog.Discard).text = qsTr("Close forever");
    }
    onAccepted: sessionIds.forEach(id => Models.SessionActions.hide(id))
    onDiscarded: {
        sessionIds.forEach(id => Models.SessionActions.close(id));
        close();
    }
}
