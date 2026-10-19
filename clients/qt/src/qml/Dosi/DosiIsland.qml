pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

AbstractButton {
    id: root

    readonly property bool unseen: Models.Dosi.unseen
    readonly property bool asking: Models.Dosi.asking
    property bool cuedUnseen: false
    property bool cuedAsking: false
    property int reacts: 0

    function cueUp() {
        if (asking && !cuedAsking)
            faceCue.rings += 1
        else if (unseen && !cuedUnseen && !asking)
            reacts += 1
        cuedAsking = asking
        cuedUnseen = unseen
    }

    implicitWidth: 40
    implicitHeight: 40
    hoverEnabled: true
    activeFocusOnTab: true
    objectName: "sidebar.dosi"
    Accessible.id: objectName
    Accessible.name: "Dosi"
    Accessible.description: Models.Dosi.line
    ToolTip.visible: hovered
    ToolTip.text: Models.Dosi.line
    ToolTip.delay: Tip.delay
    scale: pressed ? 0.95 : 1

    Component.onCompleted: {
        cuedAsking = asking
        cuedUnseen = unseen
    }
    onUnseenChanged: Qt.callLater(cueUp)
    onAskingChanged: Qt.callLater(cueUp)
    onClicked: Models.DesktopState.openDosi()

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Rectangle {
        radius: KodosiTheme.radiusLg
        color: KodosiTheme.alpha(KodosiTheme.ink, root.hovered || root.visualFocus ? 0.06 : 0)
        opacity: boxSpot.hidden ? 0 : 1

        FoldSpot {
            id: boxSpot
            anchors.fill: parent
            key: "box"
            place: "rail"
            fill: KodosiTheme.ground
            radius: KodosiTheme.radiusLg
        }
    }
    contentItem: Item {
        DosiFace {
            id: face

            readonly property point look: Fold.look(face, !Models.Dosi.busy)

            anchors.centerIn: parent
            size: 20
            mood: Models.Dosi.mood
            lookX: look.x
            lookY: look.y
            reacts: root.reacts
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
                radius: 6
                ringColor: KodosiTheme.accent
            }
            StatusSign {
                x: parent.width - width / 2 + 2
                y: -height / 2 - 2
                visible: root.asking
                size: 9
                session: ({ sign: "question", asks: true, signLabel: qsTr("Dosi has a question") })
            }
            NewDot {
                x: parent.width - width / 2 + 1
                y: -height / 2 - 1
                visible: !root.asking && root.unseen
            }
        }
    }

    KFocusIndicator { active: root.visualFocus }
}
