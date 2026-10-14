pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Shapes
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property real radius: KodosiTheme.radiusLg
    property color fill: KodosiTheme.raised
    property int elevation: 1
    property bool rim: true

    readonly property real corner: Math.max(0, Math.min(radius, width / 2, height / 2))
    readonly property int level: Math.max(0, Math.min(elevation, 3))
    readonly property var shadows: [
        [{ blur: 0, x: 0, y: 0, dark: 0, light: 0 }, { blur: 0, x: 0, y: 0, dark: 0, light: 0 }],
        [{ blur: 6, x: 2, y: 4, dark: 0.3, light: 0.08 }, { blur: 3, x: 1, y: 2, dark: 0.42, light: 0.13 }],
        [{ blur: 13, x: 4, y: 8, dark: 0.42, light: 0.14 }, { blur: 8, x: 2, y: 5, dark: 0.5, light: 0.18 }],
        [{ blur: 22, x: 6, y: 13, dark: 0.5, light: 0.18 }, { blur: 12, x: 3, y: 8, dark: 0.55, light: 0.22 }]
    ][level]

    Repeater {
        model: 2

        Models.SoftShadow {
            required property int index
            readonly property var shadow: root.shadows[index]

            x: shadow.x
            y: shadow.y
            width: root.width
            height: root.height
            visible: root.level > 0
            radius: root.corner
            blur: shadow.blur
            color: KodosiTheme.shadow
            opacity: KodosiTheme.isDark ? shadow.dark : shadow.light
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: root.corner
        color: root.fill

        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
    }
    Shape {
        anchors.fill: parent
        visible: root.rim && root.width > 2 && root.height > 2
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeWidth: -1
            strokeColor: "transparent"
            fillRule: ShapePath.OddEvenFill
            fillGradient: LinearGradient {
                x1: 0
                y1: 0
                x2: root.width
                y2: root.height
                GradientStop { position: 0; color: KodosiTheme.highlight }
                GradientStop { position: 0.45; color: KodosiTheme.alpha(KodosiTheme.highlight, 0) }
                GradientStop { position: 0.6; color: KodosiTheme.alpha(KodosiTheme.lowlight, 0) }
                GradientStop { position: 1; color: KodosiTheme.lowlight }
            }
            PathRectangle { width: root.width; height: root.height; radius: root.corner }
            PathRectangle { x: 1; y: 1; width: root.width - 2; height: root.height - 2; radius: Math.max(0, root.corner - 1) }
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: root.corner
        color: "transparent"
        border.width: 0.5
        border.color: KodosiTheme.alpha(KodosiTheme.hairline, KodosiTheme.isDark ? 0.5 : 0.7)
    }
}
