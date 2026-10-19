import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property real size: 11

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: KodosiTheme.hairline
            strokeWidth: 1.5
            fillColor: "transparent"
            PathAngleArc {
                centerX: root.size / 2
                centerY: root.size / 2
                radiusX: root.size / 2 - 0.75
                radiusY: radiusX
                startAngle: 0
                sweepAngle: 360
            }
        }
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: 800
            loops: Animation.Infinite
            running: !KodosiTheme.reduceMotion && KodosiTheme.shown && root.visible
        }

        ShapePath {
            strokeColor: KodosiTheme.accent
            strokeWidth: 1.5
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: root.size / 2
                centerY: root.size / 2
                radiusX: root.size / 2 - 0.75
                radiusY: radiusX
                startAngle: -90
                sweepAngle: 100
            }
        }
    }
}
