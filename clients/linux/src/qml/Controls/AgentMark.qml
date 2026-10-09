import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property string program: ""
    property real size: 22
    property var session: null
    property bool asleep: false
    property bool working: session ? session.working === true : false
    property bool asks: session ? ["hand", "question", "key"].indexOf(session.sign) >= 0 : false
    property int progress: session && session.progress !== undefined ? session.progress : -1
    readonly property bool measured: working && progress >= 0
    readonly property bool pale: asleep && !asks
    readonly property int form: working ? 1 : asks ? 2 : 0
    readonly property string kind: Identity.kind(program)
    readonly property color tint: Identity.kindTint(kind)
    readonly property real rim: Math.max(1.5, size * 0.07)
    property real fraction: Math.max(0.02, Math.min(1, progress / 100))
    property real squash: 1
    property bool ready: false

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true
    transform: Scale { origin.x: root.width / 2; origin.y: root.height; xScale: 2 - root.squash; yScale: root.squash }

    Component.onCompleted: ready = true
    onFormChanged: {
        if (ready && !KodosiTheme.reduceMotion)
            pulse.restart()
    }
    Behavior on fraction { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
    SequentialAnimation {
        id: pulse
        NumberAnimation { target: root; property: "squash"; to: 0.9; duration: 100; easing.type: Easing.OutQuad }
        NumberAnimation { target: root; property: "squash"; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: 2.4 }
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: -root.rim * 2
        radius: root.size * 0.3 + root.rim * 2
        color: "transparent"
        border.width: root.rim
        border.color: KodosiTheme.accent
        opacity: root.asks && !root.working ? 1 : 0
        scale: root.asks && !root.working ? 1 : 1.3
        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
    }
    Shape {
        id: arc
        visible: root.measured
        anchors.fill: parent
        anchors.margins: -root.rim
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: "transparent"
            strokeWidth: 0
            fillGradient: ConicalGradient {
                centerX: arc.width / 2
                centerY: arc.height / 2
                angle: 90
                GradientStop { position: 0; color: KodosiTheme.alpha(KodosiTheme.glowOrange, 0.22) }
                GradientStop { position: Math.max(0, 0.999 - root.fraction); color: KodosiTheme.alpha(KodosiTheme.glowOrange, 0.22) }
                GradientStop { position: 1 - root.fraction; color: KodosiTheme.glowRose }
                GradientStop { position: 1 - root.fraction / 2; color: KodosiTheme.glowOrange }
                GradientStop { position: 1; color: KodosiTheme.glowAmber }
            }
            PathRectangle { width: arc.width; height: arc.height; radius: root.size * 0.3 + root.rim }
        }
    }
    Shape {
        id: glow
        visible: root.working && !root.measured
        anchors.fill: parent
        anchors.margins: -root.rim
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: "transparent"
            strokeWidth: 0
            fillGradient: ConicalGradient {
                id: sweep
                centerX: glow.width / 2
                centerY: glow.height / 2
                GradientStop { position: 0; color: KodosiTheme.glowAmber }
                GradientStop { position: 0.3; color: KodosiTheme.glowOrange }
                GradientStop { position: 0.55; color: KodosiTheme.glowRose }
                GradientStop { position: 0.8; color: KodosiTheme.alpha(KodosiTheme.glowOrange, 0.15) }
                GradientStop { position: 1; color: KodosiTheme.glowAmber }
            }
            PathRectangle { width: glow.width; height: glow.height; radius: root.size * 0.3 + root.rim }
        }
        NumberAnimation {
            target: sweep
            property: "angle"
            from: 360
            to: 0
            duration: 3200
            loops: Animation.Infinite
            running: glow.visible && !KodosiTheme.reduceMotion
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: root.size * 0.3
        gradient: Gradient {
            GradientStop { position: 0; color: root.pale ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.14) : KodosiTheme.mix(root.tint, "#ffffff", 0.2) }
            GradientStop { position: 0.5; color: root.pale ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.22) : root.tint }
            GradientStop { position: 1; color: root.pale ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.3) : KodosiTheme.mix(root.tint, "#000000", 0.2) }
        }
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, root.pale ? 0.06 : 0.18)
    }
    Image {
        anchors.centerIn: parent
        visible: root.kind !== "shell"
        width: Math.round(root.size * 0.6)
        height: width
        sourceSize.width: width * 2
        sourceSize.height: height * 2
        opacity: root.pale ? 0.6 : 1
        source: root.kind === "shell" ? "" : "qrc:/qt/qml/Kodosi/qml/assets/providers/" + root.kind + (root.kind === "claude" ? "-mark.svg" : "-dark.svg")
    }
    Row {
        anchors.centerIn: parent
        visible: root.kind === "shell"
        spacing: root.size * 0.03

        KIcon {
            width: root.size * 0.44
            height: width
            name: "prompt"
            strokeWidth: 3.2
            color: root.pale ? KodosiTheme.mix(root.tint, KodosiTheme.ink, 0.45) : "#ffffff"
        }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: root.size * 0.15
            height: root.size * 0.36
            radius: root.size * 0.03
            color: root.pale ? KodosiTheme.alpha(KodosiTheme.accent, 0.6) : KodosiTheme.mix(KodosiTheme.accent, "#ffffff", 0.25)
        }
    }
}
