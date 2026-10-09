pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Item {
    id: root

    property real radius: KodosiTheme.radiusLg
    property color fill: KodosiTheme.raised
    property int elevation: 1
    property bool rim: true

    readonly property int layers: elevation <= 0 ? 0 : elevation === 1 ? 3 : elevation === 2 ? 5 : 7
    readonly property real step: elevation === 1 ? 1.5 : elevation === 2 ? 2.8 : 4.2
    readonly property real drop: elevation === 1 ? 2 : elevation === 2 ? 5 : 9

    Repeater {
        model: root.layers

        Rectangle {
            required property int index
            readonly property real grow: (index + 1) * root.step

            x: -grow + (root.drop + index * root.step) * 0.4
            y: -grow + root.drop + index * root.step * 0.9
            width: root.width + grow * 2
            height: root.height + grow * 2
            radius: Math.min(root.radius + grow, height / 2)
            color: KodosiTheme.shadow
            opacity: (root.elevation === 1 ? 0.1 : 0.085) * KodosiTheme.shadowStrength
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        visible: root.rim
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.tint(Qt.tint(root.fill, KodosiTheme.highlight), KodosiTheme.alpha(KodosiTheme.hairline, 0.35)) }
            GradientStop { position: 1; color: Qt.tint(Qt.tint(root.fill, KodosiTheme.lowlight), KodosiTheme.alpha(KodosiTheme.hairline, 0.45)) }
        }
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: root.rim ? 1 : 0
        radius: Math.max(0, root.radius - (root.rim ? 1 : 0))
        color: root.fill

        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
    }
}
