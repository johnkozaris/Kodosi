pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property string mood: "resting"
    property real size: 22
    property bool still: false
    property real lookX: 0
    property real lookY: 0
    property int reacts: 0

    readonly property bool moving: !still && !KodosiTheme.reduceMotion && KodosiTheme.shown && visible
    readonly property real eyeWidth: size * 0.15
    readonly property real eyeHeight: size * 0.3
    readonly property real gap: size * 0.12
    readonly property var pose: {
        switch (mood) {
        case "listening": return { x: 0, y: 0.05, left: 1.08, right: 1.08, tilt: 0, opacity: 1 }
        case "thinking": return { x: 0.06, y: -0.06, left: 0.75, right: 1, tilt: 0, opacity: 1 }
        case "reading": return { x: 0.07, y: 0.01, left: 1, right: 1, tilt: 0, opacity: 1 }
        case "working": return { x: 0.05, y: 0.06, left: 0.8, right: 0.8, tilt: 0, opacity: 1 }
        case "asking": return { x: 0, y: -0.02, left: 1.12, right: 1.12, tilt: 0.14, opacity: 1 }
        case "stuck": return { x: 0, y: 0.04, left: 0.22, right: 0.22, tilt: 0, opacity: 0.75 }
        case "off": return { x: 0, y: 0, left: 0.18, right: 0.18, tilt: 0, opacity: 0.35 }
        default: return { x: 0, y: 0, left: 1, right: 1, tilt: 0, opacity: 1 }
        }
    }

    property real gazeX: pose.x
    property real gazeY: pose.y
    property real glanceX: still ? 0 : lookX * 0.09
    property real glanceY: still ? 0 : lookY * 0.07
    property real tilt: pose.tilt
    property real leftScale: pose.left
    property real rightScale: pose.right
    property real fade: pose.opacity
    property real closing: mood === "stopping" && moving ? 0.2 : 1
    property real blink: 1
    property real ponder: 0
    property real ponderX: 0
    property real ponderY: 0
    property real nod: 0
    property real scanX: 0
    property real scanY: 0
    property real workX: 0
    property real workY: 0

    implicitWidth: size
    implicitHeight: size
    Accessible.role: Accessible.Graphic
    Accessible.name: "Dosi"
    Accessible.description: Models.Dosi.moodLabel(mood)

    onReactsChanged: {
        if (moving)
            react.restart()
    }

    Behavior on gazeX { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on gazeY { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on glanceX { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
    Behavior on glanceY { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
    Behavior on tilt { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
    Behavior on leftScale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on rightScale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on fade { NumberAnimation { duration: KodosiTheme.motionFade } }
    Behavior on closing { NumberAnimation { duration: KodosiTheme.reduceMotion ? 0 : 350; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve } }
    Behavior on ponder { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on ponderX { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on ponderY { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on nod { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on scanX { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on scanY { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on workX { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    Behavior on workY { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

    Squircle {
        anchors.fill: parent
        lightColor: KodosiTheme.terminal
        darkColor: KodosiTheme.terminal
        rimLight: 0.18
        rimShade: 0.3
        shadow: KodosiTheme.isDark ? 0.45 : 0.2
    }
    Item {
        x: (root.gazeX + root.glanceX + root.ponderX + root.scanX + root.workX) * root.size
        y: (root.gazeY + root.glanceY + root.nod + root.ponderY + root.scanY + root.workY) * root.size
        width: root.size
        height: root.size
        rotation: root.tilt * 180 / Math.PI
        opacity: root.fade

        Repeater {
            model: 2

            Rectangle {
                id: eye

                required property int index
                readonly property real stretch: (index === 0 ? root.leftScale + root.ponder : root.rightScale) * root.closing * root.blink

                x: root.size / 2 + (index === 0 ? -1 : 1) * (root.gap / 2 + root.eyeWidth / 2) - root.eyeWidth / 2
                y: (root.size - root.eyeHeight) / 2
                width: root.eyeWidth
                height: root.eyeHeight
                radius: root.eyeWidth * 0.3
                color: "#db8a62"
                transform: Scale { origin.x: eye.width / 2; origin.y: eye.height / 2; yScale: eye.stretch }
            }
        }
    }
    SequentialAnimation {
        id: react
        onStopped: root.blink = 1
        NumberAnimation { target: root; property: "blink"; to: 0.15; duration: 108 }
        NumberAnimation { target: root; property: "blink"; to: 1.08; duration: 144; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        NumberAnimation { target: root; property: "blink"; to: 1; duration: 108; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
    }
    SequentialAnimation {
        running: root.moving && root.mood === "talking"
        loops: Animation.Infinite
        onStopped: root.nod = 0
        NumberAnimation { target: root; property: "nod"; to: 0.02; duration: 800; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "nod"; to: 0; duration: 800; easing.type: Easing.InOutSine }
    }
    ParallelAnimation {
        running: root.moving && root.mood === "thinking"
        loops: Animation.Infinite
        onStopped: {
            root.ponderX = 0
            root.ponderY = 0
            root.ponder = 0
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "ponderX"; to: -0.035; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "ponderX"; to: 0.02; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "ponderX"; to: 0; duration: 1200; easing.type: Easing.InOutSine }
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "ponderY"; to: -0.025; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "ponderY"; to: 0.015; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "ponderY"; to: 0; duration: 1200; easing.type: Easing.InOutSine }
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "ponder"; to: 0.18; duration: 1800; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "ponder"; to: 0; duration: 1800; easing.type: Easing.InOutSine }
        }
    }
    ParallelAnimation {
        running: root.moving && root.mood === "reading"
        loops: Animation.Infinite
        onStopped: {
            root.scanX = 0
            root.scanY = 0
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "scanX"; to: 0.07; duration: 1600; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "scanX"; to: 0; duration: 1600; easing.type: Easing.InOutSine }
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "scanY"; to: 0.02; duration: 3200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "scanY"; to: 0; duration: 3200; easing.type: Easing.InOutSine }
        }
    }
    ParallelAnimation {
        running: root.moving && root.mood === "working"
        loops: Animation.Infinite
        onStopped: {
            root.workX = 0
            root.workY = 0
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "workY"; to: 0.025; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "workY"; to: 0; duration: 1200; easing.type: Easing.InOutSine }
        }
        SequentialAnimation {
            NumberAnimation { target: root; property: "workX"; to: 0.03; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "workX"; to: 0; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "workX"; to: -0.03; duration: 1200; easing.type: Easing.InOutSine }
            NumberAnimation { target: root; property: "workX"; to: 0; duration: 1200; easing.type: Easing.InOutSine }
        }
    }
}
