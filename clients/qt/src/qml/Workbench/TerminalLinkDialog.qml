import Kodosi 1.0
import QtQuick
import QtQuick.Controls

KDialog {
    id: root

    property url link
    property string target: ""
    property string reason: ""
    readonly property bool refused: reason.length > 0

    signal copyRequested(string target)

    function ask(asked, shown) {
        if (visible)
            return;
        link = asked;
        target = shown;
        reason = "";
        open();
    }

    function refuse(shown, why) {
        if (visible)
            return;
        target = shown;
        reason = why;
        open();
    }

    standardButtons: refused ? Dialog.Ok | Dialog.Apply : Dialog.Ok | Dialog.Cancel
    title: refused ? qsTr("Kodosi does not open this link") : qsTr("Open this link in another app?")
    onOpened: {
        if (refused)
            standardButton(Dialog.Apply).text = qsTr("Copy Link");
        else
            standardButton(Dialog.Ok).text = qsTr("Open");
    }
    onAccepted: {
        if (!refused)
            Qt.openUrlExternally(link);
    }
    onApplied: {
        copyRequested(target);
        close();
    }

    Column {
        width: 380
        spacing: 10

        PlainLabel {
            width: parent.width
            visible: root.refused
            text: root.reason
            color: KodosiTheme.inkMuted
            wrapMode: Text.WordWrap
        }
        PlainLabel {
            width: parent.width
            text: root.target
            wrapMode: Text.WrapAnywhere
        }
        PlainLabel {
            width: parent.width
            visible: !root.refused
            text: qsTr("The link comes from terminal output. Open it only if you trust it.")
            color: KodosiTheme.inkMuted
            wrapMode: Text.WordWrap
        }
    }
}
