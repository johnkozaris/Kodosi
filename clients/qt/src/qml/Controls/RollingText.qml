import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string text: ""
    property alias font: current.font
    property color color: KodosiTheme.ink
    property string shown: ""
    property string previous: ""
    property bool rising: true
    property real progress: 1
    property bool ready: false

    implicitWidth: current.implicitWidth
    implicitHeight: current.implicitHeight
    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Component.onCompleted: {
        shown = text
        ready = true
    }
    onTextChanged: {
        if (!ready || !visible || KodosiTheme.reduceMotion) {
            roll.stop()
            progress = 1
            shown = text
            return
        }
        previous = shown
        shown = text
        const before = previous.match(/\d+/g) || []
        const after = shown.match(/\d+/g) || []
        const index = after.findIndex((number, position) => number !== before[position])
        rising = index < 0 || index >= before.length || Number(after[index]) >= Number(before[index])
        roll.restart()
    }

    Text {
        id: current
        width: root.width
        height: root.height
        y: (1 - root.progress) * root.implicitHeight * (root.rising ? 0.6 : -0.6)
        opacity: root.progress
        text: root.shown
        textFormat: Text.PlainText
        color: root.color
        verticalAlignment: Text.AlignVCenter
        font.features: { "tnum": 1 }
    }
    Text {
        width: root.width
        height: root.height
        y: root.progress * root.implicitHeight * (root.rising ? -0.6 : 0.6)
        visible: root.progress < 1
        opacity: 1 - root.progress
        text: root.previous
        textFormat: Text.PlainText
        color: root.color
        font: current.font
        verticalAlignment: Text.AlignVCenter
    }
    NumberAnimation {
        id: roll
        target: root
        property: "progress"
        from: 0
        to: 1
        duration: KodosiTheme.motionSoft
        easing.type: Easing.OutCubic
    }
}
