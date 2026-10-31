pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    property string recordingAction: ""
    property string recordingKey: ""
    property string note: ""

    function record(action, key) {
        recordingAction = action;
        recordingKey = key;
        note = key.length > 0 ? qsTr("Press a key. Delete removes this one.") : qsTr("Press a key.");
        recorder.forceActiveFocus();
    }
    function stop() {
        recordingAction = "";
        recordingKey = "";
    }
    function receive(event) {
        event.accepted = true;
        if (event.key === Qt.Key_Escape) {
            note = "";
            stop();
        } else if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
            if (recordingKey.length > 0)
                Models.Shortcuts.remove(recordingAction, recordingKey);
            note = "";
            stop();
        } else {
            const sequence = Models.Shortcuts.sequence(event.key, event.modifiers);
            if (sequence.length === 0)
                return;
            if (recordingAction === "numberedTerminal" && !/[0-9]$/.test(sequence)) {
                note = qsTr("Use a digit.");
                return;
            }
            const holder = Models.Shortcuts.bind(recordingAction, sequence, recordingKey);
            note = holder.length > 0 ? qsTr("%1 was %2.").arg(sequence).arg(holder) : "";
            stop();
        }
    }

    spacing: 18

    Item {
        id: recorder

        Layout.preferredWidth: 0
        Layout.preferredHeight: 0
        Keys.enabled: root.recordingAction.length > 0
        Keys.onPressed: event => root.receive(event)
        Keys.onShortcutOverride: event => event.accepted = true
        onActiveFocusChanged: {
            if (!activeFocus)
                root.stop();
        }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 12

        PlainLabel {
            Layout.fillWidth: true
            text: root.note
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
            elide: Text.ElideRight
        }
        KButton {
            compact: true
            enabled: Models.Shortcuts.changed
            objectName: "panel.settings.keyboard.reset"
            Accessible.id: objectName
            text: qsTr("Reset all")
            onClicked: {
                root.note = "";
                Models.Shortcuts.reset();
            }
        }
    }
    Repeater {
        model: 3

        ListGroup {
            id: group

            required property int index

            Layout.fillWidth: true

            Repeater {
                model: Models.Shortcuts.rows.filter(row => row.group === group.index)

                ListRow {
                    id: row

                    required property var modelData

                    title: modelData.title

                    Repeater {
                        model: row.modelData.keys

                        KButton {
                            id: cap

                            required property var modelData

                            compact: true
                            variant: root.recordingAction === row.modelData.action && root.recordingKey === modelData.sequence
                                ? KButton.Tinted : KButton.Secondary
                            objectName: "panel.settings.keyboard." + row.modelData.action + "." + modelData.sequence
                            Accessible.id: objectName
                            text: modelData.label
                            onClicked: root.record(row.modelData.action, modelData.sequence)
                        }
                    }
                    KIconButton {
                        glyph: "plus"
                        objectName: "panel.settings.keyboard." + row.modelData.action + ".add"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Add a key for %1").arg(row.modelData.title)
                        onClicked: root.record(row.modelData.action, "")
                    }
                }
            }
        }
    }
}
