import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

Item {
    id: root

    property bool shown

    implicitWidth: row.implicitWidth + 26
    implicitHeight: 34
    opacity: shown ? 1 : 0
    visible: opacity > 0
    Accessible.id: objectName
    Accessible.role: Accessible.StaticText
    Accessible.name: label.text
    transform: Translate {
        y: root.shown ? 0 : -8

        Behavior on y { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
    }

    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

    Raised {
        anchors.fill: parent
        radius: height / 2
        fill: KodosiTheme.terminalLifted
        dark: true
        elevation: 2
    }
    RowLayout {
        id: row

        anchors.verticalCenter: parent.verticalCenter
        x: 12
        spacing: 8

        BreathingDot { color: KodosiTheme.glowOrange }
        PlainLabel {
            id: label

            text: qsTr("Reconnecting")
            color: KodosiTheme.terminalInk
        }
    }
}
