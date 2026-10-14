import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property real radius: KodosiTheme.radiusXl
    property bool hovered: false

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: KodosiTheme.alpha(KodosiTheme.accent, root.hovered ? 0.07 : 0)

        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: "transparent"
            strokeWidth: 1.2
            strokeColor: root.hovered ? KodosiTheme.alpha(KodosiTheme.accent, 0.7) : KodosiTheme.alpha(KodosiTheme.inkFaint, 0.5)
            strokeStyle: ShapePath.DashLine
            dashPattern: [5 / 1.2, 5 / 1.2]
            PathRectangle {
                x: 0.6
                y: 0.6
                width: Math.max(0, root.width - 1.2)
                height: Math.max(0, root.height - 1.2)
                radius: Math.max(0, root.radius - 0.6)
            }
        }
    }
}
