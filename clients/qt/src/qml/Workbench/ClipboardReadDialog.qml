import Kodosi 1.0
import QtQuick
import QtQuick.Controls

KDialog {
    id: root

    property string sessionName: ""
    property bool paste: false

    signal answered(bool allowed)

    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Paste the clipboard into %1?").arg(sessionName)
    onAboutToShow: paste = false
    onOpened: {
        standardButton(Dialog.Ok).text = qsTr("Paste");
        standardButton(Dialog.Cancel).text = qsTr("Don't Paste");
    }
    onAccepted: paste = true
    // The terminal takes input again only once the dialog has closed.
    onClosed: answered(paste)

    PlainLabel {
        width: 340
        text: qsTr("The program in this terminal asks for the text on your clipboard.")
        color: KodosiTheme.inkMuted
        wrapMode: Text.WordWrap
    }
}
