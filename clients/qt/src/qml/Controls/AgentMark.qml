pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.impl
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property string program: ""
    property real size: 22
    property var session: null
    property bool asleep: false
    property bool working: session ? session.working === true : false
    property int progress: session && session.progress !== undefined ? session.progress : -1
    readonly property bool asRoot: session ? session.root === true : false
    readonly property bool measured: working && progress >= 0
    readonly property string kind: Models.DesktopSettings.agentOf(program)
    readonly property color tint: Identity.kindTint(kind)
    readonly property real rim: Math.max(2, size * 0.075)
    readonly property real space: rim * 1.75
    property real fraction: Math.max(0.02, Math.min(1, progress / 100))
    property real squash: 1
    property bool ready: false

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true
    transform: Scale { origin.x: root.width / 2; origin.y: root.height; xScale: 2 - root.squash; yScale: root.squash }

    Component.onCompleted: ready = true
    onWorkingChanged: {
        if (ready && !KodosiTheme.reduceMotion)
            pulse.restart()
    }
    Behavior on fraction { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
    SequentialAnimation {
        id: pulse
        NumberAnimation { target: root; property: "squash"; to: 0.9; duration: 100; easing.type: Easing.OutQuad }
        NumberAnimation { target: root; property: "squash"; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: 2.4 }
    }

    Loader {
        anchors.fill: parent
        anchors.margins: -root.space
        active: root.working

        sourceComponent: CellRim {
            radius: root.size * 0.3 + root.space
            lineWidth: root.rim
            mode: root.measured ? "progress" : "working"
            fraction: root.fraction

            NumberAnimation on opacity { from: 0; to: 1; duration: KodosiTheme.motionFade }
        }
    }
    Squircle {
        anchors.fill: parent
        lightColor: root.asleep ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.14) : KodosiTheme.mix(root.tint, "#ffffff", 0.2)
        baseColor: root.asleep ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.22) : root.tint
        darkColor: root.asleep ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.3) : KodosiTheme.mix(root.tint, "#000000", 0.2)
        rimLight: root.asleep ? 0.12 : 0.4
        shadow: root.asleep ? 0 : KodosiTheme.isDark ? 0.45 : 0.2
    }
    IconImage {
        anchors.centerIn: parent
        visible: root.kind !== "shell"
        width: Math.round(root.size * 0.6)
        height: width
        sourceSize.width: width * 2
        sourceSize.height: height * 2
        color: root.asleep ? KodosiTheme.mix(root.tint, KodosiTheme.ink, 0.35) : "#ffffff"
        source: root.kind === "shell" ? "" : "qrc:/qt/qml/Kodosi/qml/assets/providers/" + root.kind + (root.kind === "claude" ? "-mark.svg" : "-dark.svg")
    }
    Row {
        anchors.centerIn: parent
        visible: root.kind === "shell"
        spacing: root.size * 0.03

        KIcon {
            visible: !root.asRoot
            width: root.size * 0.44
            height: width
            name: "prompt"
            strokeWidth: 4.2
            color: root.asleep ? KodosiTheme.mix(root.tint, KodosiTheme.ink, 0.35) : "#ffffff"
        }
        Text {
            visible: root.asRoot
            anchors.verticalCenter: parent.verticalCenter
            text: "#"
            textFormat: Text.PlainText
            color: root.asleep ? KodosiTheme.alpha(KodosiTheme.terminalCaution, 0.7) : KodosiTheme.terminalCaution
            font.pixelSize: Math.max(1, Math.round(root.size * 0.46))
            font.weight: Font.Black
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
