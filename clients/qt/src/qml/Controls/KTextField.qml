import Kodosi 1.0
import QtQuick
import QtQuick.Controls

TextField {
    id: root

    implicitHeight: KodosiTheme.fieldHeight
    leftPadding: 12
    rightPadding: 12
    topPadding: 7
    bottomPadding: 7
    color: enabled ? KodosiTheme.ink : KodosiTheme.inkFaint
    placeholderTextColor: KodosiTheme.inkFaint
    selectionColor: KodosiTheme.accent
    selectedTextColor: KodosiTheme.accentInk
    font.pixelSize: KodosiTheme.fontBody
    selectByMouse: true

    background: Well {
        radius: KodosiTheme.radiusMd
        border.width: root.activeFocus ? 1.5 : 1
        border.color: root.activeFocus ? KodosiTheme.alpha(KodosiTheme.accent, 0.75) : Qt.tint(KodosiTheme.well, KodosiTheme.alpha(KodosiTheme.isDark ? "#000000" : KodosiTheme.hairline, KodosiTheme.isDark ? 0.4 : 0.6))
    }
}
