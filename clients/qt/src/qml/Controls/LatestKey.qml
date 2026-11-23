import Kodosi 1.0
import QtQuick

KButton {
    id: root

    property bool shown: false

    anchors.horizontalCenter: parent.horizontalCenter
    anchors.bottom: parent.bottom
    anchors.bottomMargin: shown ? 8 : -height
    compact: true
    iconName: "chevron-down"
    text: qsTr("Latest")
    opacity: shown ? 1 : 0
    visible: opacity > 0
    Accessible.id: objectName

    Behavior on anchors.bottomMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
}
