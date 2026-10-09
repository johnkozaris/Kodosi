import Kodosi 1.0
import QtQuick

Rectangle {
    radius: KodosiTheme.radiusLg
    color: KodosiTheme.well
    border.width: 1
    border.color: Qt.tint(KodosiTheme.well, KodosiTheme.alpha(KodosiTheme.isDark ? "#000000" : KodosiTheme.hairline, KodosiTheme.isDark ? 0.4 : 0.6))
}
