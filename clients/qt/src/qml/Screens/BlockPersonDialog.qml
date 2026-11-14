pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property string username: ""
    property string fullName: ""

    function ask(handle, name) {
        username = handle;
        fullName = name;
        open();
    }

    destructive: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Block %1?").arg(fullName)
    onOpened: standardButton(Dialog.Ok).text = qsTr("Block")

    onAccepted: Models.People.block(username)

    PlainLabel {
        width: 340
        color: KodosiTheme.inkMuted
        text: qsTr("They cannot send you friend requests. If you are friends, you stop sharing terminals with each other, and each of you leaves the projects that the other owns.")
        wrapMode: Text.WordWrap
    }
}
