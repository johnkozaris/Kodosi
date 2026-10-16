import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Shape {
    id: root

    property real radius: Math.min(width, height) * 0.3
    property color lightColor: KodosiTheme.raised
    property color baseColor: KodosiTheme.mix(lightColor, darkColor, 0.5)
    property color darkColor: KodosiTheme.raised
    property real rim: Math.max(0.5, Math.min(width, height) * 0.03)
    property real rimLight: 0.4
    property real rimShade: 0.18
    property real shadow: 0
    property real shadowRadius: Math.min(width, height) * 0.1
    property real shadowX: width * 0.03
    property real shadowY: height * 0.07

    function shade(spread: real): string {
        return KodosiTheme.squircle(shadowX - spread * 0.6, shadowY - spread * 0.3,
            width + spread * 1.6, height + spread * 1.5, radius + spread)
    }

    preferredRendererType: Shape.CurveRenderer

    ShapePath {
        strokeColor: "transparent"
        fillColor: KodosiTheme.alpha(KodosiTheme.shadow, root.shadow * 0.08)

        PathSvg { path: root.shade(root.shadowRadius) }
    }
    ShapePath {
        strokeColor: "transparent"
        fillColor: KodosiTheme.alpha(KodosiTheme.shadow, root.shadow * 0.14)

        PathSvg { path: root.shade(root.shadowRadius * 0.66) }
    }
    ShapePath {
        strokeColor: "transparent"
        fillColor: KodosiTheme.alpha(KodosiTheme.shadow, root.shadow * 0.2)

        PathSvg { path: root.shade(root.shadowRadius * 0.33) }
    }
    ShapePath {
        strokeColor: "transparent"
        fillColor: KodosiTheme.alpha(KodosiTheme.shadow, root.shadow * 0.3)

        PathSvg { path: root.shade(0) }
    }
    ShapePath {
        strokeWidth: root.rim
        fillGradient: LinearGradient {
            x2: root.width
            y2: root.height
            GradientStop { position: 0; color: root.lightColor }
            GradientStop { position: 0.5; color: root.baseColor }
            GradientStop { position: 1; color: root.darkColor }
        }
        strokeGradient: LinearGradient {
            x2: root.width
            y2: root.height
            GradientStop { position: 0; color: Qt.rgba(1, 1, 1, root.rimLight) }
            GradientStop { position: 0.5; color: Qt.rgba(1, 1, 1, 0) }
            GradientStop { position: 0.5; color: Qt.rgba(0, 0, 0, 0) }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, root.rimShade) }
        }

        PathSvg { path: KodosiTheme.squircle(root.rim / 2, root.rim / 2, root.width - root.rim, root.height - root.rim, root.radius - root.rim / 2) }
    }
}
