pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Row {
    id: root

    property int level: -1
    property int count: 0
    property real barHeight: 11
    property real barWidth: 2.5

    height: barHeight
    spacing: barWidth * 0.6
    Accessible.ignored: true

    Repeater {
        model: root.count

        Rectangle {
            required property int index

            anchors.bottom: parent ? parent.bottom : undefined
            width: root.barWidth
            height: root.barHeight * (0.35 + 0.65 * index / Math.max(root.count - 1, 1))
            radius: root.barWidth * 0.4
            color: index <= root.level ? KodosiTheme.accent : KodosiTheme.alpha(KodosiTheme.ink, 0.2)

            Behavior on color {
                SequentialAnimation {
                    PauseAnimation { duration: KodosiTheme.reduceMotion ? 0 : index * 35 }
                    ColorAnimation { duration: KodosiTheme.motionSnappy }
                }
            }
        }
    }
}
