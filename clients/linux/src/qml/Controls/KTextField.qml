import Kodosi 1.0
import QtQuick
import QtQuick.Controls

TextField {
    id: root

    implicitHeight: KodosiTheme.controlHeight
    leftPadding: 11
    rightPadding: 11
    topPadding: 7
    bottomPadding: 7
    color: enabled ? KodosiTheme.textPrimary : KodosiTheme.disabled
    placeholderTextColor: KodosiTheme.placeholderText
    selectionColor: KodosiTheme.accent
    selectedTextColor: KodosiTheme.accentForeground
    font.pixelSize: KodosiTheme.fontBody
    selectByMouse: true

    KFocusIndicator { active: root.activeFocus && root.enabled }

    background: Rectangle {
        color: KodosiTheme.input
        radius: KodosiTheme.radiusSmall
    }
}
