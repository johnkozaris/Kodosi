import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property string program: ""
    property real size: 22
    property bool asleep: false
    property bool working: false
    readonly property string kind: Identity.kind(program)
    readonly property color tint: Identity.kindTint(kind)
    readonly property real rim: Math.max(1.5, size * 0.07)

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Shape {
        id: glow
        visible: root.working
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
            GradientStop { position: 0; color: root.asleep ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.14) : KodosiTheme.mix(root.tint, "#ffffff", 0.2) }
            GradientStop { position: 0.5; color: root.asleep ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.22) : root.tint }
            GradientStop { position: 1; color: root.asleep ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.3) : KodosiTheme.mix(root.tint, "#000000", 0.2) }
        }
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, root.asleep ? 0.06 : 0.18)
    }
    Image {
        anchors.centerIn: parent
        visible: root.kind !== "shell"
        width: Math.round(root.size * 0.6)
        height: width
        sourceSize.width: width * 2
        sourceSize.height: height * 2
        opacity: root.asleep ? 0.6 : 1
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
            color: root.asleep ? KodosiTheme.mix(root.tint, KodosiTheme.ink, 0.45) : "#ffffff"
        }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: root.size * 0.15
            height: root.size * 0.36
            radius: root.size * 0.03
            color: root.asleep ? KodosiTheme.alpha(KodosiTheme.accent, 0.6) : KodosiTheme.mix(KodosiTheme.accent, "#ffffff", 0.25)
        }
    }
}
