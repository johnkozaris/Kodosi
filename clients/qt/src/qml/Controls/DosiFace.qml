pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property string mood: "resting"
    property real size: 22
    property bool still: false

    readonly property bool moving: !still && !KodosiTheme.reduceMotion && KodosiTheme.shown && visible
    readonly property real eyeWidth: size * 0.15
    readonly property real eyeHeight: size * 0.3
    readonly property real gap: size * 0.12
    readonly property var pose: {
        switch (mood) {
        case "listening": return { x: 0, y: 0.05, left: 1.08, right: 1.08, tilt: 0, opacity: 1 }
        case "thinking": return { x: 0.06, y: -0.06, left: 0.75, right: 1, tilt: 0, opacity: 1 }
        case "reading": return { x: -0.07, y: -0.03, left: 1, right: 1, tilt: 0, opacity: 1 }
        case "working": return { x: 0, y: 0.06, left: 0.8, right: 0.8, tilt: 0, opacity: 1 }
        case "asking": return { x: 0, y: -0.02, left: 1.12, right: 1.12, tilt: 0.14, opacity: 1 }
        case "stuck": return { x: 0, y: 0.04, left: 0.22, right: 0.22, tilt: 0, opacity: 0.75 }
        case "off": return { x: 0, y: 0, left: 0.18, right: 0.18, tilt: 0, opacity: 0.35 }
        default: return { x: 0, y: 0, left: 1, right: 1, tilt: 0, opacity: 1 }
        }
    }
    readonly property int blinkPeriod: mood === "resting" || mood === "asking" ? 4200
        : mood === "listening" ? 3200 : mood === "talking" ? 3600 : 0

    property real gazeX: pose.x
    property real gazeY: pose.y
    property real tilt: pose.tilt
    property real leftScale: pose.left
    property real rightScale: pose.right
    property real fade: pose.opacity
    property real closing: mood === "stopping" && moving ? 0.2 : 1
    property real blink: 1
    property real ponder: 0
    property real driftX: 0
    property real driftY: 0
    property real breath: 0
    property real typing: 1

    implicitWidth: size
    implicitHeight: size
    Accessible.role: Accessible.Graphic
    Accessible.name: "Dosi"
    Accessible.description: Models.Dosi.moodLabel(mood)

    Behavior on gazeX { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on gazeY { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on tilt { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutBack } }
    Behavior on leftScale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on rightScale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on fade { NumberAnimation { duration: KodosiTheme.motionFade } }
    Behavior on closing { NumberAnimation { duration: KodosiTheme.reduceMotion ? 0 : 500; easing.type: Easing.InQuad } }

    Squircle {
        anchors.fill: parent
        lightColor: KodosiTheme.terminal
        darkColor: KodosiTheme.terminal
        rimLight: 0.18
        rimShade: 0.3
        shadow: KodosiTheme.isDark ? 0.45 : 0.2
    }
    Item {
        x: (root.gazeX + root.driftX) * root.size
        y: (root.gazeY + root.driftY) * root.size
        width: root.size
        height: root.size
        rotation: root.tilt * 180 / Math.PI
        opacity: Math.max(0, root.fade + root.breath)

        Repeater {
            model: 2

            Rectangle {
                id: eye

                required property int index
                readonly property real stretch: (index === 0 ? root.leftScale + root.ponder : root.rightScale) * root.blink * root.closing

                x: root.size / 2 + (index === 0 ? -1 : 1) * (root.gap / 2 + root.eyeWidth / 2) - root.eyeWidth / 2
                y: (root.size - root.eyeHeight) / 2
                width: root.eyeWidth
                height: root.eyeHeight
                radius: root.eyeWidth * 0.3
                color: "#db8a62"
                opacity: root.typing
                transform: Scale { origin.x: eye.width / 2; origin.y: eye.height / 2; yScale: eye.stretch }
            }
        }
    }
    SequentialAnimation {
        running: root.moving && root.blinkPeriod > 0
        loops: Animation.Infinite
        onStopped: root.blink = 1
        PauseAnimation { duration: root.blinkPeriod * 0.94 }
        NumberAnimation { target: root; property: "blink"; to: 0.1; duration: root.blinkPeriod * 0.03 }
        NumberAnimation { target: root; property: "blink"; to: 1; duration: root.blinkPeriod * 0.03 }
    }
    SequentialAnimation {
        running: root.moving && root.mood === "talking"
        loops: Animation.Infinite
        onStopped: root.driftY = 0
        NumberAnimation { target: root; property: "driftY"; to: 0.025; duration: 450; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "driftY"; to: 0; duration: 450; easing.type: Easing.InOutSine }
    }
    ParallelAnimation {
        running: root.moving && root.mood === "thinking"
        loops: Animation.Infinite
        onStopped: {
            root.driftX = 0
            root.driftY = 0
            root.ponder = 0
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "driftX"; to: -0.03; duration: 930; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftX"; to: 0.01; duration: 930; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftX"; to: 0; duration: 940; easing.type: Easing.InOutSine }
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "driftY"; to: -0.02; duration: 930; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftY"; to: 0.02; duration: 930; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftY"; to: 0; duration: 940; easing.type: Easing.InOutSine }
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "ponder"; to: 0.25; duration: 1400; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "ponder"; to: 0; duration: 1400; easing.type: Easing.InOutSine }
        }
    }
    ParallelAnimation {
        running: root.moving && root.mood === "reading"
        loops: Animation.Infinite
        onStopped: {
            root.driftX = 0
            root.driftY = 0
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "driftX"; to: 0.14; duration: 756; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftX"; to: 0; duration: 135; easing.type: Easing.OutCubic }
            NumberAnimation { target: root; property: "driftX"; to: 0.14; duration: 756; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftX"; to: 0; duration: 135; easing.type: Easing.OutCubic }
            NumberAnimation { target: root; property: "driftX"; to: 0.14; duration: 756; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "driftX"; to: 0; duration: 162; easing.type: Easing.OutCubic }
        }
        SequentialAnimation {
            PauseAnimation { duration: 810 }
            NumberAnimation { target: root; property: "driftY"; to: 0.04; duration: 81 }
            PauseAnimation { duration: 810 }
            NumberAnimation { target: root; property: "driftY"; to: 0.08; duration: 81 }
            PauseAnimation { duration: 810 }
            NumberAnimation { target: root; property: "driftY"; to: 0; duration: 108 }
        }
    }
    SequentialAnimation {
        running: root.moving && root.mood === "working"
        loops: Animation.Infinite
        onStopped: root.typing = 1
        NumberAnimation { target: root; property: "typing"; to: 0.3; duration: 80 }
        PauseAnimation { duration: 360 }
        NumberAnimation { target: root; property: "typing"; to: 1; duration: 80 }
        PauseAnimation { duration: 480 }
    }
    SequentialAnimation {
        running: root.moving && root.mood === "stuck"
        loops: Animation.Infinite
        onStopped: root.breath = 0
        NumberAnimation { target: root; property: "breath"; to: -0.3; duration: 1500; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "breath"; to: 0; duration: 1500; easing.type: Easing.InOutSine }
    }
}
