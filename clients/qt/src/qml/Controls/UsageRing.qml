import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property var limit: null
    property real lineWidth: 2
    property bool onTerminal: false
    readonly property real used: limit ? Math.max(0, Math.min(100, limit.used)) / 100 : 0
    readonly property color tone: !limit ? "transparent"
        : limit.level === "reached" ? (onTerminal ? KodosiTheme.terminalDanger : KodosiTheme.danger)
        : limit.level === "near" ? (onTerminal ? KodosiTheme.terminalCaution : KodosiTheme.caution)
        : onTerminal ? KodosiTheme.terminalInkMuted : KodosiTheme.inkMuted
    property real drawn: 0

    implicitWidth: 14
    implicitHeight: 14
    Accessible.ignored: true

    Component.onCompleted: drawn = used
    onUsedChanged: drawn = used

    Behavior on drawn { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: KodosiTheme.alpha(root.onTerminal ? KodosiTheme.terminalInk : KodosiTheme.ink, root.limit ? 0.12 : 0.24)
            strokeWidth: root.lineWidth
            strokeStyle: root.limit ? ShapePath.SolidLine : ShapePath.DashLine
            dashPattern: [1.1, 1.5]
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
            strokeColor: root.limit && root.limit.used > 0 ? root.tone : "transparent"
            strokeWidth: root.lineWidth
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.width / 2 - root.lineWidth / 2
                radiusY: radiusX
                startAngle: -90
                sweepAngle: 360 * root.drawn
            }
        }
    }
}
