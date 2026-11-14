import Kodosi 1.0
import QtQuick

Row {
    id: root

    property bool shown: false

    visible: opacity > 0
    opacity: shown ? 1 : 0
    scale: shown ? 1 : 0.9
    spacing: 4

    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade; easing.type: Easing.OutCubic } }
    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

    KIcon {
        anchors.verticalCenter: parent.verticalCenter
        width: 12
        height: 12
        name: "check"
        color: KodosiTheme.ready
    }
    PlainLabel {
        anchors.verticalCenter: parent.verticalCenter
        text: qsTr("Kodosi got your report.")
        color: KodosiTheme.ready
        font.pixelSize: KodosiTheme.fontFootnote
    }
}
