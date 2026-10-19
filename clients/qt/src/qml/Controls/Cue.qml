import Kodosi 1.0
import QtQuick

Item {
    id: root

    property int glints: 0
    property int rings: 0
    property int delay: 0
    property real radius: KodosiTheme.radiusSm
    property color ringColor: KodosiTheme.glowAmber

    Accessible.ignored: true

    onGlintsChanged: {
        if (!KodosiTheme.reduceMotion && KodosiTheme.shown)
            glow.restart()
    }
    onRingsChanged: {
        if (!KodosiTheme.reduceMotion && KodosiTheme.shown)
            pulse.restart()
    }

    Rectangle {
        id: glint
        anchors.fill: parent
        radius: root.radius
        color: KodosiTheme.glowAmber
        opacity: 0
    }
    Rectangle {
        id: ring
        anchors.fill: parent
        radius: root.radius
        color: "transparent"
        border.width: 2
        border.color: root.ringColor
        opacity: 0
    }
    SequentialAnimation {
        id: glow
        PauseAnimation { duration: root.delay }
        OpacityAnimator { target: glint; from: 0; to: 0.4; duration: 140; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        OpacityAnimator { target: glint; from: 0.4; to: 0; duration: 500; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
    }
    ParallelAnimation {
        id: pulse
        ScaleAnimator { target: ring; from: 1; to: 1.5; duration: 600; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        OpacityAnimator { target: ring; from: 0.9; to: 0; duration: 600; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
    }
}
