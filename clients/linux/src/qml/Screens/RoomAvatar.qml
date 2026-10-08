import QtQuick
import Kodosi 1.0
Rectangle {
    id: root
    property string name: ""
    implicitWidth: 28
    implicitHeight: 28
    radius: width / 2
    color: KodosiTheme.surfaceSelected
    Accessible.name: name
    PlainLabel {
        anchors.centerIn: parent
        text: root.name.split(" ").filter(part => part.length > 0).slice(0, 2).map(part => part[0]).join("").toUpperCase()
        font.pixelSize: root.width * 0.36
        font.weight: Font.DemiBold
        color: KodosiTheme.accent
    }
}
