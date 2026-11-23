import Kodosi 1.0
import QtQuick

Rectangle {
    property Look look: KodosiTheme.look

    radius: KodosiTheme.radiusLg
    color: look.well
    border.width: 1
    border.color: Qt.tint(look.well, KodosiTheme.alpha(look.dark ? "#000000" : look.hairline, look.dark ? 0.4 : 0.6))
}
