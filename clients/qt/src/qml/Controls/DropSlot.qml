import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

// What a place shows while something that it takes is dragged over it.
Item {
    id: root

    property bool shown: false
    property real radius: KodosiTheme.radiusMd
    property bool dashed: true
    property string label: ""

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: KodosiTheme.alpha(KodosiTheme.accentSoft, root.dashed && root.shown ? 0.55 : 0)
        border.width: root.dashed ? 0 : 2
        border.color: root.shown ? KodosiTheme.accent : "transparent"
        Behavior on color { ColorAnimation { duration: KodosiTheme.motionSnappy } }
    }
    Shape {
        anchors.fill: parent
        visible: root.dashed && root.shown
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
    Column {
        anchors.centerIn: parent
        spacing: 10
        visible: root.label.length > 0
        opacity: root.shown ? 1 : 0
        scale: root.shown || KodosiTheme.reduceMotion ? 1 : 0.6

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 48
            height: 48
            radius: 24
            color: KodosiTheme.accent

            KIcon {
                anchors.centerIn: parent
                width: 18
                height: 18
                name: "chevron-down"
                strokeWidth: 2.6
                color: KodosiTheme.accentInk
            }
        }
        PlainLabel {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.label
            font.weight: Font.DemiBold
        }
    }
}
