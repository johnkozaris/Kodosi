import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    objectName: "panel.settings.account.confirmation"
    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Sign out?")

    onAccepted: Models.Account.logout()
    onOpened: standardButton(Dialog.Ok).text = qsTr("Sign out")

    PlainLabel {
        color: KodosiTheme.inkMuted
        text: qsTr("Sharing stops. The terminals on this computer keep running.")
        width: 340
        wrapMode: Text.WordWrap
    }
}
