import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Label {
    id: root

    color: KodosiTheme.ink
    font.pixelSize: KodosiTheme.fontBody
    textFormat: Text.PlainText
    transform: Translate { y: -KodosiTheme.lift(root.font.pixelSize) }
}
