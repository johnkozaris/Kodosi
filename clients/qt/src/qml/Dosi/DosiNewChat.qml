pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KIconButton {
    id: root

    glyph: "pencil"
    active: confirm.visible
    enabled: Models.Dosi.state !== "unavailable"
    objectName: "dosi.new"
    Accessible.id: objectName
    Accessible.name: qsTr("New chat")
    onClicked: confirm.open()

    KPopover {
        id: confirm

        x: root.width - width
        y: root.height + 8
        width: 280
        padding: 14
        focus: true
        transformOrigin: Popup.TopRight
        objectName: "dosi.new.confirm"
        onOpened: cancel.forceActiveFocus(Qt.PopupFocusReason)

        contentItem: ColumnLayout {
            spacing: 10
            Keys.onReturnPressed: confirm.close()
            Keys.onEnterPressed: confirm.close()

            PlainLabel {
                text: qsTr("Start a new chat?")
                color: KodosiTheme.ink
                font.pixelSize: KodosiTheme.fontHeadline
                font.weight: Font.DemiBold
            }
            PlainLabel {
                Layout.fillWidth: true
                text: qsTr("Dosi forgets this chat, here and on your other computers.")
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                wrapMode: Text.WordWrap
            }
            RowLayout {
                spacing: 6

                Item { Layout.fillWidth: true }
                KButton {
                    id: cancel
                    compact: true
                    text: qsTr("Cancel")
                    objectName: "dosi.new.cancel"
                    Accessible.id: objectName
                    onClicked: confirm.close()
                }
                KButton {
                    compact: true
                    variant: KButton.Danger
                    text: qsTr("Start new chat")
                    objectName: "dosi.new.start"
                    Accessible.id: objectName
                    onClicked: {
                        confirm.close()
                        Models.Dosi.renew()
                    }
                }
            }
        }
    }
}
