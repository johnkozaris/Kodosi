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
    readonly property bool dimmed: asleep && !working
    readonly property string kind: Models.DesktopSettings.agentOf(program)
    readonly property color tint: Identity.kindTint(kind)
    readonly property real edge: Math.max(1.5, size * 0.07)

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Squircle {
        anchors.fill: parent
        lightColor: root.dimmed ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.14) : KodosiTheme.mix(root.tint, "#ffffff", 0.2)
        baseColor: root.dimmed ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.22) : root.tint
        darkColor: root.dimmed ? KodosiTheme.mix(KodosiTheme.raised, root.tint, 0.3) : KodosiTheme.mix(root.tint, "#000000", 0.2)
        rimLight: root.dimmed ? 0.12 : 0.4
        shadow: root.dimmed ? 0 : KodosiTheme.isDark ? 0.45 : 0.2
    }
    Loader {
        anchors.fill: parent
        opacity: root.working && !root.measured ? 1 : 0
        active: opacity > 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

        sourceComponent: LightRim {
            tile: true
            radius: root.size * 0.3
            lineWidth: root.edge
        }
    }
    Loader {
        anchors.fill: parent
        opacity: root.measured ? 1 : 0
        active: opacity > 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

        sourceComponent: LightRim {
            id: meter

            tile: true
            mode: "progress"
            radius: root.size * 0.3
            lineWidth: root.edge

            Behavior on fraction { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
            Binding {
                target: meter
                property: "fraction"
                value: Math.min(1, root.progress / 100)
                when: root.measured
                restoreMode: Binding.RestoreNone
            }
        }
    }
    IconImage {
        anchors.centerIn: parent
        visible: root.kind !== "shell"
        width: Math.round(root.size * 0.6)
        height: width
        sourceSize.width: width * 2
        sourceSize.height: height * 2
        color: root.dimmed ? KodosiTheme.mix(root.tint, KodosiTheme.ink, 0.35) : "#ffffff"
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
            color: root.dimmed ? KodosiTheme.mix(root.tint, KodosiTheme.ink, 0.35) : "#ffffff"
        }
        Text {
            visible: root.asRoot
            anchors.verticalCenter: parent.verticalCenter
            text: "#"
            textFormat: Text.PlainText
            color: root.dimmed ? KodosiTheme.alpha(KodosiTheme.terminalCaution, 0.7) : KodosiTheme.terminalCaution
            font.pixelSize: Math.max(1, Math.round(root.size * 0.46))
            font.weight: Font.Black
        }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: root.size * 0.15
            height: root.size * 0.36
            radius: root.size * 0.03
            color: root.dimmed ? KodosiTheme.alpha(KodosiTheme.accent, 0.6) : KodosiTheme.mix(KodosiTheme.accent, "#ffffff", 0.25)
        }
    }
}
