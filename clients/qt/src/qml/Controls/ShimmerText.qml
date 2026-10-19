pragma ComponentBehavior: Bound

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
    readonly property real sweep: Math.max(width, 1)
    property string shown: ""
    property real arrival: 1
    property bool ready: false

    implicitWidth: base.implicitWidth
    implicitHeight: base.implicitHeight
    Accessible.ignored: true
    transform: Translate { y: (1 - root.arrival) * 4 }

    Component.onCompleted: {
        shown = text
        ready = true
    }
    onTextChanged: {
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
        duration: KodosiTheme.motionSoft
        easing.type: Easing.OutCubic
    }

    Text {
        id: base
        anchors.fill: parent
        opacity: root.arrival
        color: root.color
        textFormat: Text.PlainText
        verticalAlignment: Text.AlignVCenter
    }
    Item {
        id: window
        visible: root.active && !KodosiTheme.reduceMotion && KodosiTheme.shown
        opacity: root.arrival
        height: parent.height
        width: root.band
        clip: true

        Text {
            id: sheen
            width: base.width
            height: base.height
            text: base.text
            font: base.font
            elide: base.elide
            color: Qt.lighter(root.color, 1.5)
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
        }
    }
    SequentialAnimation {
        running: window.visible
        loops: Animation.Infinite
        ParallelAnimation {
            XAnimator { target: window; from: -root.band; to: root.sweep; duration: 1700; easing.type: Easing.InOutSine }
            XAnimator { target: sheen; from: root.band; to: -root.sweep; duration: 1700; easing.type: Easing.InOutSine }
        }
        XAnimator { target: window; from: root.sweep; to: root.sweep; duration: 250 }
    }
}
