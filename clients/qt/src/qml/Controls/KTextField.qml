import Kodosi 1.0
import QtQuick
import QtQuick.Controls

TextField {
    id: root

    property bool compact: false

    implicitHeight: compact ? 26 : KodosiTheme.fieldHeight
    leftPadding: compact ? 10 : 12
    rightPadding: compact ? 10 : 12
    topPadding: compact ? 4 : 7
    bottomPadding: compact ? 4 : 7
    color: enabled ? KodosiTheme.ink : KodosiTheme.inkFaint
    placeholderTextColor: KodosiTheme.inkFaint
    selectionColor: KodosiTheme.accent
    selectedTextColor: KodosiTheme.accentInk
    font.pixelSize: compact ? KodosiTheme.fontFootnote : KodosiTheme.fontBody
    selectByMouse: true

    background: Well {
        radius: root.compact ? KodosiTheme.radiusSm : KodosiTheme.radiusMd
        border.width: root.activeFocus ? 1.5 : 1
        border.color: root.activeFocus ? KodosiTheme.alpha(KodosiTheme.accent, 0.75) : Qt.tint(KodosiTheme.well, KodosiTheme.alpha(KodosiTheme.isDark ? "#000000" : KodosiTheme.hairline, KodosiTheme.isDark ? 0.4 : 0.6))
    }
}
