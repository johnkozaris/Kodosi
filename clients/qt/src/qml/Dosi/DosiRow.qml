pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

AbstractButton {
    id: root

    readonly property string line: Models.Dosi.state === "unavailable" ? Models.Dosi.status
        : Models.Dosi.busy ? (Models.Dosi.deed.length > 0 ? Models.Dosi.deed : Models.Dosi.moodLabel(Models.Dosi.mood).toLowerCase())
        : Models.Dosi.asking ? qsTr("waits for you") : ""
    readonly property bool unseen: Models.Dosi.unseen
    readonly property bool asking: Models.Dosi.asking

    implicitHeight: 40
    hoverEnabled: true
    activeFocusOnTab: true
    objectName: "sidebar.dosi"
    Accessible.id: objectName
    Accessible.name: "Dosi"
    Accessible.description: line
    scale: pressed ? 0.98 : 1

    onUnseenChanged: {
        if (unseen)
            cue.glints += 1
    }
    onAskingChanged: {
        if (asking)
            faceCue.rings += 1
    }
    onClicked: Models.DesktopState.openDosi()

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Item {
        Rectangle {
            id: box
            anchors.fill: parent
            radius: KodosiTheme.radiusMd
            color: KodosiTheme.alpha(KodosiTheme.ink, root.hovered || root.visualFocus ? 0.075 : 0.045)
            opacity: boxSpot.hidden ? 0 : 1

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }

            FoldSpot {
                id: boxSpot
                anchors.fill: parent
                key: "box"
                place: "row"
                fill: box.color
                radius: box.radius
            }
        }
        Cue {
            id: cue
            anchors.fill: parent
            radius: KodosiTheme.radiusMd
        }
    }
    contentItem: RowLayout {
        spacing: 10

        DosiFace {
            Layout.leftMargin: 8
            size: 24
            mood: Models.Dosi.mood
            opacity: faceSpot.hidden ? 0 : 1

            FoldSpot {
                id: faceSpot
                anchors.fill: parent
                key: "face"
                place: "row"
            }
            Cue {
                id: faceCue
                anchors.fill: parent
                radius: 7.2
                ringColor: KodosiTheme.accent
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            opacity: arrive.progress
            transform: Arrive { id: arrive; delay: 100; rise: 0 }
            onVisibleChanged: {
                if (visible)
                    arrive.play()
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1

                PlainLabel {
                    Layout.fillWidth: true
                    text: "Dosi"
                    color: KodosiTheme.ink
                    font.weight: Font.DemiBold
                }
                ShimmerText {
                    Layout.fillWidth: true
                    visible: root.line.length > 0
                    text: root.line
                    active: Models.Dosi.busy
                    color: KodosiTheme.inkFaint
                    font.family: "monospace"
                    font.pixelSize: KodosiTheme.fontCaption2
                    elide: Text.ElideRight
                }
            }
            StatusSign {
                visible: Models.Dosi.asking
                session: ({ sign: "question", asks: true, signLabel: qsTr("Dosi asks you") })
            }
            NewDot {
                visible: !Models.Dosi.asking && Models.Dosi.unseen
            }
            PlainLabel {
                Layout.rightMargin: 10
                visible: root.line.length === 0 || root.hovered
                text: qsTr("Ctrl+Shift+J")
                color: KodosiTheme.alpha(KodosiTheme.inkFaint, root.hovered ? 1 : 0.7)
                font.pixelSize: KodosiTheme.fontCaption
            }
        }
    }

    KFocusIndicator { active: root.visualFocus }
}
