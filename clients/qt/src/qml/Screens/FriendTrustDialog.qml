pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property string text: ""
    property var request: null
    readonly property string friendHandle: request ? request.handle || "" : handleIn(text)
    readonly property string name: request ? request.displayName || request.handle || "" : friendHandle.length > 0 ? "@" + friendHandle : ""
    readonly property string firstName: request ? name.split(" ")[0] : name

    signal added()

    function handleIn(value) {
        const trimmed = value.trim()
        const parts = trimmed.split(":")
        const found = parts.length === 3 && parts[0] === "kodosi" ? parts[1] : trimmed.startsWith("@") ? trimmed.slice(1) : trimmed
        return /^[A-Za-z0-9._-]{1,64}$/.test(found) ? found : ""
    }
    function askToAdd(value) {
        request = null
        text = value
        open()
    }
    function askToAccept(entry) {
        request = entry
        text = ""
        open()
    }

    standardButtons: Dialog.Ok | Dialog.Cancel
    objectName: "panel.friendTrust"
    onOpened: {
        standardButton(Dialog.Ok).text = firstName.length > 0 ? qsTr("I trust %1").arg(firstName) : qsTr("I trust them")
        standardButton(Dialog.Cancel).forceActiveFocus()
    }
    onAccepted: {
        if (request) {
            Models.People.accept(request.handle)
        } else {
            Models.People.request(text)
            root.added()
        }
    }

    ColumnLayout {
        width: 360
        spacing: 12

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: 10
            spacing: 10

            PersonAvatar { name: Models.Account.displayName || qsTr("You"); isSelf: true; size: 40 }
            Rectangle { implicitWidth: 18; implicitHeight: 2; radius: 1; color: KodosiTheme.hairline }
            Rectangle {
                implicitWidth: 80
                implicitHeight: 48
                radius: KodosiTheme.radiusMd
                color: KodosiTheme.terminal
                border.width: 1
                border.color: KodosiTheme.terminalHairline

                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    x: 12
                    spacing: 6
                    KIcon { width: 10; height: 10; anchors.verticalCenter: parent.verticalCenter; name: "chevron-down"; rotation: -90; color: KodosiTheme.inkFaint }
                    CursorBlock { width: 7; height: 13; blinks: true }
                }
            }
            Rectangle { implicitWidth: 18; implicitHeight: 2; radius: 1; color: KodosiTheme.hairline }
            PersonAvatar { name: root.friendHandle.length > 0 ? root.friendHandle : "?"; key: root.request ? root.request.userId || "" : ""; size: 40 }
        }
        Tag {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Stop and read")
            iconName: "hand"
            tone: Tag.Caution
        }
        PlainLabel {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: root.name.length > 0 ? qsTr("Do you trust %1 completely?").arg(root.name) : qsTr("Do you trust this person completely?")
            font.pixelSize: KodosiTheme.fontTitle
            font.weight: Font.DemiBold
        }
        PlainLabel {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: qsTr("A friend can type in each terminal that you share with them, as you, with your rights on that computer.")
            color: KodosiTheme.inkMuted
        }
        PlainLabel {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: qsTr("Add only a person that you would let use your computer.")
            font.weight: Font.DemiBold
        }
    }
}
