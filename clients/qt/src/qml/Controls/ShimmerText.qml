pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Effects

Item {
    id: root

    property alias text: base.text
    property alias font: base.font
    property alias elide: base.elide
    property color color: KodosiTheme.ink
    property bool active: false
    property bool blurs: false
    readonly property real band: Math.max(base.paintedWidth * 0.45, 36)
    readonly property bool softens: blurs && arrival < 1 && GraphicsInfo.api !== GraphicsInfo.Software
    property string shown: ""
    property real arrival: 1
    property bool ready: false

    implicitWidth: base.implicitWidth
    implicitHeight: base.implicitHeight
    Accessible.ignored: true
    transform: Translate { y: root.blurs ? 0 : (1 - root.arrival) * 4 }

    Component.onCompleted: {
        shown = text
        ready = true
    }
    onTextChanged: {
        ghost.text = blurs ? shown : ""
        shown = text
        if (ready && visible && !KodosiTheme.reduceMotion)
            arrive.restart()
    }
    NumberAnimation {
        id: arrive
        target: root
        property: "arrival"
        from: 0
        to: 1
        duration: root.blurs ? KodosiTheme.motionSoft + 120 : KodosiTheme.motionSoft
        easing.type: Easing.OutCubic
    }

    Text {
        id: ghost
        anchors.fill: parent
        visible: root.blurs && root.arrival < 1
        opacity: 1 - root.arrival
        scale: 1 - (1 - opacity) * 0.08
        color: root.color
        font: base.font
        elide: base.elide
        textFormat: Text.PlainText
        verticalAlignment: Text.AlignVCenter
        layer.enabled: root.softens
        layer.effect: MultiEffect {
            blurEnabled: true
            blurMax: 12
            blur: root.arrival
        }
    }
    Text {
        id: base
        anchors.fill: parent
        opacity: root.arrival
        scale: root.blurs ? 0.92 + root.arrival * 0.08 : 1
        color: root.color
        textFormat: Text.PlainText
        verticalAlignment: Text.AlignVCenter
        layer.enabled: root.softens
        layer.effect: MultiEffect {
            blurEnabled: true
            blurMax: 12
            blur: 1 - root.arrival
        }
    }
    Item {
        id: window
        visible: root.active && !KodosiTheme.reduceMotion && KodosiTheme.shown
        opacity: root.arrival
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
            color: Qt.lighter(root.color, 1.5)
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
