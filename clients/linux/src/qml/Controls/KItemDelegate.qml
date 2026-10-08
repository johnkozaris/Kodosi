import Kodosi 1.0
import QtQuick
import QtQuick.Controls

ItemDelegate {
    id: root

    hoverEnabled: true
    leftPadding: 10
    rightPadding: 10

    KFocusIndicator { active: root.activeFocus && root.enabled }

    background: Rectangle {
        color: root.down
            ? KodosiTheme.surfaceSelected
            : root.highlighted || root.checked
              ? KodosiTheme.surfaceSelected
              : root.hovered
                ? KodosiTheme.surfaceElevated
                : KodosiTheme.surface
        radius: KodosiTheme.radiusSmall
    }
}
