import Kodosi 1.0
import QtQuick
import QtQuick.Controls

KDialog {
    id: root

    property string sessionName: ""
    property bool allow: false

    signal answered(bool allowed)

    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Let a program read your clipboard?")
    onAboutToShow: allow = false
    onOpened: {
        standardButton(Dialog.Ok).text = qsTr("Allow");
        standardButton(Dialog.Cancel).text = qsTr("Don\u2019t Allow");
    }
    onAccepted: allow = true
    onClosed: answered(allow)

    PlainLabel {
        width: 340
        text: qsTr("A program in %1 asks for the text that you copied.").arg(root.sessionName)
        color: KodosiTheme.inkMuted
        wrapMode: Text.WordWrap
    }
}
