import Kodosi 1.0
import QtQuick

KButton {
    id: root

    required property string glyph
    property int size: KodosiTheme.iconButtonSize
    property color glyphColor: KodosiTheme.textSecondary

    text: ""
    iconName: glyph
    iconColor: !enabled
        ? KodosiTheme.disabled
        : checked
          ? KodosiTheme.accent
          : hovered
            ? KodosiTheme.textPrimary
            : glyphColor
    variant: KButton.Quiet
    compact: true
    implicitWidth: size
    implicitHeight: size
    leftPadding: Math.max(0, (size - 15) / 2)
    rightPadding: leftPadding
    topPadding: Math.max(0, (size - 15) / 2)
    bottomPadding: topPadding

    background: Item {}
}
