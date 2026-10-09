import Kodosi 1.0
import QtQuick

Item {
    id: root

    property alias text: base.text
    property alias font: base.font
    property alias elide: base.elide
    property color color: KodosiTheme.ink
    property bool active: false
    readonly property real band: Math.max(base.paintedWidth * 0.45, 36)

    implicitWidth: base.implicitWidth
    implicitHeight: base.implicitHeight
    Accessible.ignored: true

    Text {
        id: base
        anchors.fill: parent
        color: root.color
        textFormat: Text.PlainText
        verticalAlignment: Text.AlignVCenter
    }
    Item {
        id: window
        visible: root.active && !KodosiTheme.reduceMotion
        height: parent.height
        width: root.band
        clip: true

        Text {
            x: -window.x
            width: base.width
            height: base.height
            text: base.text
            font: base.font
            elide: base.elide
            color: KodosiTheme.glowAmber
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
        }
        SequentialAnimation on x {
            running: window.visible
            loops: Animation.Infinite
            NumberAnimation { from: -root.band; to: Math.max(root.width, 1); duration: 1700; easing.type: Easing.InOutSine }
            PauseAnimation { duration: 250 }
        }
    }
}
