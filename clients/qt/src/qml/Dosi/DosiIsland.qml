pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

AbstractButton {
    id: root

    readonly property bool unseen: Models.Dosi.unseen
    readonly property bool asking: Models.Dosi.asking

    implicitWidth: Math.max(40, marks.implicitWidth + 16)
    implicitHeight: 40
    hoverEnabled: true
    activeFocusOnTab: true
    objectName: "sidebar.dosi"
    Accessible.id: objectName
    Accessible.name: "Dosi"
    Accessible.description: Models.Dosi.status
    ToolTip.visible: hovered
    ToolTip.text: Models.Dosi.status
    ToolTip.delay: 600
    scale: pressed ? 0.94 : 1

    onUnseenChanged: {
        if (unseen)
            cue.glints += 1
    }
    onAskingChanged: {
        if (asking)
            faceCue.rings += 1
    }
    onClicked: Models.DesktopState.openDosi()

    Behavior on implicitWidth { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutBack } }
    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Item {
        Raised {
            anchors.fill: parent
            radius: 12
            fill: KodosiTheme.raised
            opacity: boxSpot.hidden ? 0 : 1

            FoldSpot {
                id: boxSpot
                anchors.fill: parent
                key: "box"
                place: "rail"
                fill: KodosiTheme.raised
                radius: 12
            }
        }
        Cue {
            id: cue
            anchors.fill: parent
            radius: 12
        }
    }
    contentItem: Item {
        Row {
            id: marks
            anchors.centerIn: parent
            spacing: 6

            DosiFace {
                size: 24
                mood: Models.Dosi.mood
                opacity: faceSpot.hidden ? 0 : 1

                FoldSpot {
                    id: faceSpot
                    anchors.fill: parent
                    key: "face"
                    place: "rail"
                }
                Cue {
                    id: faceCue
                    anchors.fill: parent
                    radius: 7.2
                    ringColor: KodosiTheme.accent
                }
            }
            StatusSign {
                anchors.verticalCenter: parent.verticalCenter
                visible: Models.Dosi.asking
                size: 10
                session: ({ sign: "question", asks: true, signLabel: qsTr("Dosi asks you") })
            }
            NewDot {
                anchors.verticalCenter: parent.verticalCenter
                visible: !Models.Dosi.asking && Models.Dosi.unseen
            }
        }
    }

    KFocusIndicator { active: root.visualFocus }
}
