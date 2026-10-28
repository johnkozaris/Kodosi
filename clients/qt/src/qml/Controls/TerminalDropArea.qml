import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

DropArea {
    id: root

    property real radius: KodosiTheme.radiusMd
    property bool dashed: true

    signal terminalDropped(string sessionId)

    keys: ["application/x-kodosi-terminal"]
    onDropped: drop => {
        const id = drop.getDataAsString("application/x-kodosi-terminal")
        if (id.length === 0)
            return
        drop.accept(Qt.MoveAction)
        root.terminalDropped(id)
    }

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: KodosiTheme.alpha(KodosiTheme.accentSoft, root.dashed && root.containsDrag ? 0.55 : 0)
        border.width: root.dashed ? 0 : 2
        border.color: root.containsDrag ? KodosiTheme.accent : "transparent"
        Behavior on color { ColorAnimation { duration: KodosiTheme.motionSnappy } }
    }
    Shape {
        anchors.fill: parent
        visible: root.dashed && root.containsDrag
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: KodosiTheme.accent
            strokeWidth: 1.5
            strokeStyle: ShapePath.DashLine
            dashPattern: [3, 2.5]
            fillColor: "transparent"
            PathRectangle { x: 0.75; y: 0.75; width: root.width - 1.5; height: root.height - 1.5; radius: root.radius }
        }
    }
}
