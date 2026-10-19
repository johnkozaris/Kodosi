pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
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

    implicitHeight: head.y + head.height

    Component.onCompleted: {
        cuedAsking = asking
        cuedUnseen = unseen
    }
    onUnseenChanged: Qt.callLater(cueUp)
    onAskingChanged: Qt.callLater(cueUp)

    Rectangle {
        x: 8
        width: parent.width - 16
        height: 1
        color: KodosiTheme.mix(KodosiTheme.hairline, KodosiTheme.ground, 0.4)
    }
    AbstractButton {
        id: head

        y: 7
        width: parent.width
        height: 32
        hoverEnabled: true
        activeFocusOnTab: true
        scale: pressed ? 0.98 : 1
        objectName: "sidebar.dosi"
        Accessible.id: objectName
        Accessible.name: "Dosi"
        Accessible.description: Models.Dosi.line
        onClicked: Models.DesktopState.openDosi()

        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

        background: Rectangle {
            radius: KodosiTheme.radiusMd
            color: KodosiTheme.alpha(KodosiTheme.ink, head.hovered || head.visualFocus ? 0.06 : 0)
            opacity: boxSpot.hidden ? 0 : 1

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }

            FoldSpot {
                id: boxSpot
                anchors.fill: parent
                key: "box"
                place: "row"
                fill: KodosiTheme.ground
                radius: KodosiTheme.radiusMd
            }
        }
        contentItem: RowLayout {
            spacing: 9

            DosiFace {
                id: face

                readonly property point look: Fold.look(face, !Models.Dosi.busy)

                Layout.leftMargin: 7
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
                    place: "row"
                }
                Cue {
                    id: faceCue
                    anchors.fill: parent
                    radius: 6
                    ringColor: KodosiTheme.accent
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.rightMargin: 10
                spacing: 9
                opacity: arrive.progress
                transform: Arrive { id: arrive; delay: 100; rise: 0 }
                onVisibleChanged: {
                    if (visible)
                        arrive.play()
                }

                RollingText {
                    Layout.fillWidth: true
                    text: Models.Dosi.line
                    color: root.asking || root.unseen || head.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                    font.family: "monospace"
                    font.pixelSize: KodosiTheme.fontFootnote
                    elide: Text.ElideRight

                    Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                }
                Row {
                    spacing: -3

                    Repeater {
                        model: Models.Dosi.touched

                        Item {
                            id: mark

                            required property var modelData
                            readonly property var session: modelData.kind === "terminal" ? Models.Sessions.presentationForSession(modelData.id) : null

                            width: 16
                            height: 16
                            Component.onCompleted: pop.start()

                            AgentMark {
                                anchors.fill: parent
                                visible: mark.modelData.kind === "terminal"
                                size: 16
                                program: mark.session ? mark.session.program || "" : ""
                                session: mark.session
                                working: false
                            }
                            RoomSigil {
                                anchors.fill: parent
                                visible: mark.modelData.kind === "room"
                                key: mark.modelData.id
                                size: 16
                            }
                            ParallelAnimation {
                                id: pop
                                NumberAnimation { target: mark; property: "scale"; from: 0.6; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
                                NumberAnimation { target: mark; property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade }
                            }
                        }
                    }
                }
                StatusSign {
                    visible: root.asking
                    session: ({ sign: "question", asks: true, signLabel: qsTr("Dosi has a question") })
                }
                NewDot {
                    visible: !root.asking && root.unseen
                }
            }
        }

        KFocusIndicator { active: head.visualFocus }
    }
}
