import Kodosi 1.0
import QtQuick

TextEdit {
    textFormat: Text.PlainText
    readOnly: true
    selectByMouse: true
    selectByKeyboard: true
    wrapMode: Text.Wrap
    color: KodosiTheme.ink
    font.pixelSize: KodosiTheme.fontBody
    selectionColor: KodosiTheme.accent
    selectedTextColor: KodosiTheme.accentInk
}
