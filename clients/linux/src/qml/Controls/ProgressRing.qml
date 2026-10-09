import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property real progress: 0
    property color color: KodosiTheme.accent
    property color track: KodosiTheme.alpha(KodosiTheme.ink, 0.22)
    property real lineWidth: 2

    implicitWidth: 14
    implicitHeight: 14
    Accessible.ignored: true

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: root.track
            strokeWidth: root.lineWidth
            fillColor: "transparent"
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.width / 2 - root.lineWidth / 2
                radiusY: radiusX
                startAngle: 0
                sweepAngle: 360
            }
        }
        ShapePath {
            strokeColor: root.progress > 0 ? root.color : "transparent"
            strokeWidth: root.lineWidth
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.width / 2 - root.lineWidth / 2
                radiusY: radiusX
                startAngle: -90
                sweepAngle: 360 * Math.max(0, Math.min(1, root.arc))
            }
        }
    }

    property real arc: progress

    Behavior on arc { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
}
