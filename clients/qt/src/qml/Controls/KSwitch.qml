import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Switch {
    id: root

    implicitWidth: 38
    implicitHeight: 22
    padding: 0
    activeFocusOnTab: true

    indicator: Rectangle {
        width: 38
        height: 22
        radius: 11
        color: root.checked ? KodosiTheme.accent : KodosiTheme.well
        border.width: root.visualFocus ? 1.5 : 1
        border.color: root.visualFocus ? KodosiTheme.accentStrong : KodosiTheme.alpha(KodosiTheme.hairline, 0.7)

        Behavior on color { ColorAnimation { duration: KodosiTheme.motionFade } }

        Raised {
            x: root.checked ? parent.width - width - 3 : 3
            y: 3
            width: 16
            height: 16
            radius: 8
            fill: root.checked ? KodosiTheme.accentInk : KodosiTheme.isDark ? KodosiTheme.inkMuted : KodosiTheme.raised
            rim: false

            Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
        }
    }
    contentItem: Item {}
}
