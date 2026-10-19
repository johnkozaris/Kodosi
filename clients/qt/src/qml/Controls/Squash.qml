import Kodosi 1.0
import QtQuick

SequentialAnimation {
    id: root

    required property Scale shape

    ParallelAnimation {
        NumberAnimation { target: root.shape; property: "xScale"; to: 1.08; duration: 80; easing.type: Easing.OutQuad }
        NumberAnimation { target: root.shape; property: "yScale"; to: 0.9; duration: 80; easing.type: Easing.OutQuad }
    }
    ParallelAnimation {
        NumberAnimation { target: root.shape; property: "xScale"; to: 0.97; duration: 100; easing.type: Easing.InOutQuad }
        NumberAnimation { target: root.shape; property: "yScale"; to: 1.04; duration: 100; easing.type: Easing.InOutQuad }
    }
    ParallelAnimation {
        NumberAnimation { target: root.shape; property: "xScale"; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
        NumberAnimation { target: root.shape; property: "yScale"; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
    }
}
