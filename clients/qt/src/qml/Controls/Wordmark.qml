import Kodosi 1.0
import QtQuick

// The Kodosi wordmark, 1.2 times `size` tall, and the copper cursor at the end of the word: it
// stands on the line of the word, as tall as its small letters.
Row {
    id: root

    property real size: 16
    property bool blinks: false

    spacing: size * 0.1
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Kodosi")

    Image {
        height: root.size * 1.2
        width: height * 2705 / 617
        sourceSize.height: height * 2
        fillMode: Image.PreserveAspectFit
        smooth: true
        mipmap: true
        source: "qrc:/qt/qml/Kodosi/qml/assets/kodosi-wordmark.png"
    }
    CursorBlock {
        y: root.size * 0.32
        width: root.size * 0.43
        height: root.size * 0.86
        blinks: root.blinks
    }
}
