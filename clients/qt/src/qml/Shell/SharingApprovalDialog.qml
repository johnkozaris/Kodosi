pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    readonly property var question: Models.Sharing.question
    property var shown: ({})
    property bool closing: false

    function follow(): void {
        if (closing)
            return;
        if (question.approvalId === undefined) {
            if (visible)
                leaving.restart();
            return;
        }
        leaving.stop();
        if (visible && shown.approvalId === question.approvalId)
            return;
        shown = question;
        settling.restart();
        if (visible)
            message.Accessible.announce(shown.title + " " + shown.message);
        else
            open();
    }
    function answer(allow: bool): void {
        if (settling.running)
            return;
        Models.Sharing.answer(shown.approvalId, allow);
        if (question.approvalId === undefined) {
            leaving.stop();
            close();
        }
    }
    function focusCancel(): void {
        if (opened && !settling.running)
            buttons.standardButton(DialogButtonBox.Cancel).forceActiveFocus();
    }

    // The question ends only with an answer, and Escape answers Cancel: neither a click outside nor
    // a notification that brings a terminal to the front closes it.
    closePolicy: Popup.NoAutoClose
    title: shown.title ?? ""
    objectName: "sharing.approval"
    header: PlainLabel {
        padding: 22
        bottomPadding: 6
        text: root.title
        wrapMode: Text.Wrap
        font.pixelSize: KodosiTheme.fontHeadline + 2
        font.weight: Font.DemiBold
    }
    // The dialog's own button box would close it on each answer, so the next question could not
    // take the place of the last one.
    footer: Item {
        implicitWidth: buttons.implicitWidth
        implicitHeight: buttons.implicitHeight

        KDialogButtons {
            id: buttons

            anchors.fill: parent
            // A click or a key meant for what was there a moment ago does not answer a new question.
            enabled: !settling.running
            standardButtons: DialogButtonBox.Ok | DialogButtonBox.Cancel
            onAccepted: root.answer(true)
            onRejected: root.answer(false)
            Component.onCompleted: standardButton(DialogButtonBox.Ok).text = Qt.binding(() => root.shown.confirm ?? "")
        }
    }
    onQuestionChanged: follow()
    onAboutToShow: closing = false
    onAboutToHide: closing = true
    onOpened: focusCancel()
    onClosed: {
        closing = false;
        follow();
    }

    Shortcut {
        enabled: root.opened && !settling.running
        sequences: [StandardKey.Cancel]
        onActivated: root.answer(false)
    }
    Timer {
        id: settling
        interval: 500
        onTriggered: root.focusCancel()
    }
    // A question that the host sends again right away, after it connects or resends its state,
    // stays on screen.
    Timer {
        id: leaving
        interval: 300
        onTriggered: root.close()
    }

    PlainLabel {
        id: message

        width: root.availableWidth
        text: root.shown.message ?? ""
        color: KodosiTheme.inkMuted
        wrapMode: Text.Wrap
    }
}
