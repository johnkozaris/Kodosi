import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property string glyph: ""
    property string text: ""
    property color tint: KodosiTheme.accentStrong
    property bool round: true
    property real size: 26

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: KodosiTheme.alpha(root.tint, 0.8)
            strokeWidth: 1.2
            strokeStyle: ShapePath.DashLine
            dashPattern: [2.2, 1.8]
            fillColor: "transparent"

            PathSvg {
                readonly property real edge: root.size - 1.2

                path: root.round
                    ? "M 0.6 " + root.size / 2 + " A " + edge / 2 + " " + edge / 2 + " 0 1 0 " + (root.size - 0.6) + " " + root.size / 2
                        + " A " + edge / 2 + " " + edge / 2 + " 0 1 0 0.6 " + root.size / 2 + " Z"
                    : KodosiTheme.squircle(0.6, 0.6, edge, edge, root.size * 0.3)
            }
        }
    }
    KIcon {
        anchors.centerIn: parent
        visible: root.glyph.length > 0
        width: Math.round(root.size * 0.5)
        height: width
        name: root.glyph
        color: root.tint
        strokeWidth: 2.2
    }
    Text {
        anchors.centerIn: parent
        visible: root.text.length > 0
        text: root.text.toUpperCase()
        textFormat: Text.PlainText
        color: root.tint
        font.pixelSize: Math.round(root.size * 0.4)
        font.weight: Font.DemiBold
    }
}
