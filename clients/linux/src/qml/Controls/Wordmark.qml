import Kodosi 1.0
import QtQuick

Row {
    id: root

    property real size: 16
    property bool blinks: false
    property bool showsMark: true

    spacing: size * 0.5
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Kodosi")

    Image {
        anchors.verticalCenter: parent.verticalCenter
        visible: root.showsMark
        width: root.size * 1.5
        height: width
        sourceSize.width: width * 2
        sourceSize.height: height * 2
        fillMode: Image.PreserveAspectFit
        source: "qrc:/qt/qml/Kodosi/qml/assets/kodosi-logo-dark.png"
    }
    Row {
        anchors.verticalCenter: parent.verticalCenter
        spacing: root.size * 0.14

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "kodosi"
            color: KodosiTheme.ink
            font.family: "monospace"
            font.pixelSize: root.size
            font.weight: Font.Black
            textFormat: Text.PlainText
        }
        CursorBlock {
            anchors.verticalCenter: parent.verticalCenter
            width: root.size * 0.5
            height: root.size * 0.95
            blinks: root.blinks
        }
    }
}
