pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    required property Models.TerminalChat chat
    required property Look look
    property string finding: ""
    readonly property var shown: {
        const words = finding.trim().toLowerCase()
        if (words.length === 0)
            return chat.settings
        return chat.settings.filter(setting => setting.label.toLowerCase().includes(words)
            || setting.description.toLowerCase().includes(words))
    }

    spacing: 8

    KTextField {
        Layout.fillWidth: true
        compact: true
        placeholderText: qsTr("Find a setting")
        objectName: "chat.settings.find"
        Accessible.name: placeholderText
        onTextChanged: root.finding = text
    }
    Spinner {
        Layout.alignment: Qt.AlignHCenter
        Layout.topMargin: 40
        Layout.bottomMargin: 40
        visible: root.chat.settings.length === 0
    }
    PlainLabel {
        Layout.fillWidth: true
        Layout.topMargin: 12
        Layout.bottomMargin: 12
        visible: root.shown.length === 0 && root.chat.settings.length > 0
        text: qsTr("No setting matches")
        color: root.look.inkFaint
        font.pixelSize: KodosiTheme.fontFootnote
        horizontalAlignment: Text.AlignHCenter
    }
    ListView {
        id: list

        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(contentHeight, 420)
        visible: root.shown.length > 0
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: Models.KeyedList {
            key: "key"
            values: root.shown
        }
        ScrollBar.vertical: KScrollBar {}

        delegate: Item {
            id: row

            required property var modelData
            readonly property bool locked: modelData.locked === true
            readonly property string value: modelData.value

            width: ListView.view.width
            implicitHeight: rowColumn.implicitHeight + 16
            objectName: "chat.setting." + modelData.key
            onValueChanged: field.text = value

            Column {
                id: rowColumn

                x: 4
                y: 8
                width: parent.width - 8
                spacing: 4

                RowLayout {
                    width: parent.width
                    spacing: 10

                    Column {
                        Layout.fillWidth: true
                        spacing: 2

                        Row {
                            spacing: 5

                            PlainLabel {
                                text: row.modelData.label
                                color: root.look.ink
                                font.pixelSize: KodosiTheme.fontBody
                            }
                            KIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: row.locked
                                width: KodosiTheme.scaled(10)
                                height: KodosiTheme.scaled(10)
                                name: "lock"
                                color: root.look.inkFaint
                                Accessible.role: Accessible.StaticText
                                Accessible.name: qsTr("Locked")
                            }
                        }
                        PlainLabel {
                            width: parent.width
                            visible: row.modelData.description.length > 0
                            text: row.modelData.description
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            wrapMode: Text.WordWrap
                        }
                    }
                    KSwitch {
                        visible: row.modelData.kind === "toggle"
                        enabled: !row.locked
                        checkable: false
                        checked: row.value === "true"
                        Accessible.name: row.modelData.label
                        onClicked: root.chat.set(row.modelData.key, checked ? "false" : "true")
                    }
                    KPicker {
                        visible: row.modelData.kind === "choice"
                        enabled: !row.locked
                        terminalLook: root.look
                        values: row.modelData.options.map(option => ({ value: option, label: option }))
                        current: row.value
                        Accessible.name: row.modelData.label
                        onPicked: value => root.chat.set(row.modelData.key, value)
                    }
                    KTextField {
                        id: field

                        Layout.preferredWidth: 150
                        visible: row.modelData.kind === "text" || row.modelData.kind === "number"
                        enabled: !row.locked
                        compact: true
                        text: row.value
                        horizontalAlignment: TextInput.AlignRight
                        inputMethodHints: row.modelData.kind === "number" ? Qt.ImhFormattedNumbersOnly : Qt.ImhNone
                        Accessible.name: row.modelData.label
                        onAccepted: {
                            if (text !== row.value)
                                root.chat.set(row.modelData.key, text)
                        }
                    }
                }
                PlainLabel {
                    width: parent.width
                    visible: row.modelData.refusal.length > 0
                    text: row.modelData.refusal
                    color: root.look.caution
                    font.pixelSize: KodosiTheme.fontCaption
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
}
