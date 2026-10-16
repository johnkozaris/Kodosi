import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Shape {
    id: root

    property real radius: KodosiTheme.radiusXl
    property bool hovered: false

    preferredRendererType: Shape.CurveRenderer

    ShapePath {
        strokeWidth: 1.2
        strokeColor: root.hovered ? KodosiTheme.alpha(KodosiTheme.accent, 0.7) : KodosiTheme.alpha(KodosiTheme.inkFaint, 0.5)
        strokeStyle: ShapePath.DashLine
        dashPattern: [5 / 1.2, 5 / 1.2]
        fillColor: KodosiTheme.alpha(KodosiTheme.accent, root.hovered ? 0.07 : 0)

        Behavior on strokeColor { ColorAnimation { duration: KodosiTheme.motionHover } }
        Behavior on fillColor { ColorAnimation { duration: KodosiTheme.motionHover } }

        PathSvg { path: KodosiTheme.squircle(0.6, 0.6, root.width - 1.2, root.height - 1.2, root.radius - 0.6) }
    }
}
