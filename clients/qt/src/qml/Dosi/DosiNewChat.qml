pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KIconButton {
    id: root

    glyph: "pencil"
    active: confirm.opened
    enabled: Models.Dosi.state !== "unavailable"
    objectName: "dosi.new"
    Accessible.id: objectName
    Accessible.name: qsTr("New chat")
    ToolTip.visible: hovered && !confirm.opened
    onClicked: confirm.open()

    KPopover {
        id: confirm

        x: root.width - width
        y: -height - 8
        width: 264
        padding: 14
        focus: true
        transformOrigin: Popup.BottomRight
        objectName: "dosi.new.confirm"

        contentItem: ColumnLayout {
            spacing: 10

            PlainLabel {
                text: qsTr("Start a new chat?")
                color: KodosiTheme.ink
                font.pixelSize: KodosiTheme.fontHeadline
                font.weight: Font.DemiBold
            }
            PlainLabel {
                Layout.fillWidth: true
                text: qsTr("Dosi starts again with no history, here and on your other computers.")
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                wrapMode: Text.WordWrap
            }
            RowLayout {
                spacing: 6

                Item { Layout.fillWidth: true }
                KButton {
                    compact: true
                    variant: KButton.Ghost
                    text: qsTr("Cancel")
                    onClicked: confirm.close()
                }
                KButton {
                    compact: true
                    variant: KButton.Primary
                    text: qsTr("Start")
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
