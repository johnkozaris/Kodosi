pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    required property string sessionId
    property real radius: KodosiTheme.radiusTile
    readonly property var touch: Models.Dosi.touches[sessionId] || ({})
    readonly property int count: touch.count || 0
    property bool shown: false

    Accessible.ignored: true
    onCountChanged: {
        if (count === 0)
            return
        if (!KodosiTheme.reduceMotion && KodosiTheme.shown)
            flash.restart()
        shown = touch.shows === true
        if (shown)
            hide.restart()
    }

    Timer {
        id: hide
        interval: 2600
        onTriggered: root.shown = false
    }
    Item {
        id: rim
        anchors.fill: parent
        opacity: 0

        Rectangle {
            anchors.fill: parent
            radius: root.radius
            color: "transparent"
            border.width: 1.5
            border.color: KodosiTheme.glowAmber
        }
    }
    SequentialAnimation {
        id: flash
        OpacityAnimator { target: rim; from: 0; to: 1; duration: 120 }
        PauseAnimation { duration: 300 }
        OpacityAnimator { target: rim; from: 1; to: 0; duration: 900; easing.type: Easing.InOutCubic }
    }
    Item {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: 46
        anchors.rightMargin: 12
        width: card.implicitWidth + 19
        height: 34
        opacity: root.shown ? 1 : 0
        scale: root.shown ? 1 : 0.96
        visible: opacity > 0
        transformOrigin: Item.TopRight
        transform: Translate {
            y: root.shown ? 0 : -8

            Behavior on y { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
        }

        Behavior on opacity { NumberAnimation { duration: root.shown ? KodosiTheme.motionFade : KodosiTheme.motionSoft } }
        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }

        Raised {
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
            fill: KodosiTheme.terminalLifted
            dark: true
            elevation: 2
        }
        Row {
            id: card
            x: 7
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8

            DosiFace {
                anchors.verticalCenter: parent.verticalCenter
                size: 20
                mood: root.touch.running ? "working" : "resting"
                still: true
            }
            PlainLabel {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, 220)
                text: root.touch.said || ""
                color: KodosiTheme.terminalInk
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideMiddle
            }
        }
    }
}
