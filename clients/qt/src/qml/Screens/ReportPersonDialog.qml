pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property string username: ""
    property string fullName: ""
    property string failure: ""
    property bool received: false

    function ask(handle, name) {
        username = handle;
        fullName = name;
        failure = "";
        note.clear();
        open();
    }

    closePolicy: Models.People.reporting ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    contentWidth: 360
    objectName: "panel.people.report"
    title: qsTr("Report %1").arg(fullName)
    onOpened: note.forceActiveFocus()

    Connections {
        target: Models.People
        enabled: root.opened

        function onReported() {
            root.close();
            root.received = true;
            receivedTimer.restart();
        }
        function onReportFailed(message) {
            root.failure = message;
        }
    }
    Timer { id: receivedTimer; interval: 6000; onTriggered: root.received = false }
    // Dialog closes when a button box that is its footer accepts, and Report waits for the answer.
    footer: Item {
        implicitWidth: reportButtons.implicitWidth
        implicitHeight: reportButtons.implicitHeight

        DialogButtonBox {
            id: reportButtons

            anchors.fill: parent
            padding: 16
            topPadding: 4
            spacing: 8
            alignment: Qt.AlignRight
            background: Item {}

            onAccepted: {
                root.failure = "";
                Models.People.report(root.username, note.text);
            }
            onRejected: root.close()

            KButton {
                id: sendReport

                Accessible.id: objectName
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                enabled: !Models.People.reporting
                working: Models.People.reporting
                objectName: "panel.people.report.send"
                text: qsTr("Report")
                variant: KButton.Danger
            }
            KButton {
                Accessible.id: objectName
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                enabled: !Models.People.reporting
                objectName: "panel.people.report.cancel"
                text: qsTr("Cancel")
                variant: KButton.Ghost
            }
        }
    }

    ColumnLayout {
        width: parent.width
        spacing: 12

        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.inkMuted
            text: qsTr("Kodosi gets their user name and what you write here.")
            wrapMode: Text.WordWrap
        }
        Well {
            Layout.fillWidth: true
            implicitHeight: Math.max(88, note.implicitHeight + 4)
            radius: KodosiTheme.radiusMd

            TextArea {
                id: note

                anchors.fill: parent
                enabled: !Models.People.reporting
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                placeholderText: qsTr("What happened (optional)")
                color: KodosiTheme.ink
                placeholderTextColor: KodosiTheme.inkFaint
                selectionColor: KodosiTheme.accent
                selectedTextColor: KodosiTheme.accentInk
                font.pixelSize: KodosiTheme.fontBody
                background: null
                objectName: "panel.people.report.note"
                Accessible.id: objectName
                Accessible.name: qsTr("What happened (optional)")
                KeyNavigation.priority: KeyNavigation.BeforeItem
                KeyNavigation.tab: sendReport
                onTextChanged: {
                    const kept = Models.People.clippedNote(note.text);
                    if (kept.length < note.length)
                        note.remove(kept.length, note.length);
                }
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: root.failure.length > 0
            color: KodosiTheme.danger
            text: root.failure
            wrapMode: Text.WordWrap
        }
    }
}
