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
    // Blur, x, y, dark opacity, light opacity for resting, lifted and floating.
    readonly property var contact: [[0, 0, 0, 0, 0], [3, 1, 2, 0.42, 0.13], [8, 2, 5, 0.5, 0.18], [12, 3, 8, 0.55, 0.22]][level]
    readonly property var ambient: [[0, 0, 0, 0, 0], [12, 4, 8, 0.3, 0.08], [26, 8, 16, 0.42, 0.14], [44, 12, 26, 0.5, 0.18]][level]

    Models.SoftShadow {
        x: root.ambient[1]
        y: root.ambient[2]
        width: root.width
        height: root.height
        visible: root.level > 0
        radius: root.corner
        blur: root.ambient[0]
        color: KodosiTheme.shadow
        opacity: KodosiTheme.isDark ? root.ambient[3] : root.ambient[4]
    }
    Models.SoftShadow {
        x: root.contact[1]
        y: root.contact[2]
        width: root.width
        height: root.height
        visible: root.level > 0
        radius: root.corner
        blur: root.contact[0]
        color: KodosiTheme.shadow
        opacity: KodosiTheme.isDark ? root.contact[3] : root.contact[4]
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
