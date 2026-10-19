pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property bool typed: Models.Dosi.draft.trim().length > 0
    readonly property bool stops: Models.Dosi.busy && !typed
    readonly property bool focused: field.activeFocus
    readonly property var queued: Models.Dosi.details.queued || []

    function focusField() {
        field.forceActiveFocus()
    }

    implicitHeight: column.implicitHeight + 20
    objectName: "dosi.composer"

    Connections {
        function onDraftChanged() {
            if (field.text !== Models.Dosi.draft)
                field.text = Models.Dosi.draft
        }

        target: Models.Dosi
    }

    Column {
        id: column
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 6

        add: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade }
            NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack }
        }
        move: Transition {
            NumberAnimation { property: "y"; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
        }

        Repeater {
            model: root.queued

            Item {
                id: waiting

                required property var modelData

                width: column.width
                height: 26

                Rectangle {
                    anchors.fill: parent
                    radius: height / 2
                    color: KodosiTheme.alpha(KodosiTheme.ink, 0.05)
                }
                KIcon {
                    id: clock
                    x: 10
                    anchors.verticalCenter: parent.verticalCenter
                    width: 10
                    height: 10
                    name: "history"
                    color: KodosiTheme.inkFaint
                }
                PlainLabel {
                    anchors.left: clock.right
                    anchors.leftMargin: 6
                    anchors.right: remove.left
                    anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    text: waiting.modelData.text || ""
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                    elide: Text.ElideRight
                }
                KIconButton {
                    id: remove
                    anchors.right: parent.right
                    anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: "close"
                    size: 20
                    objectName: "dosi.queued.remove"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Do not send")
                    onClicked: Models.Dosi.unqueue(waiting.modelData.id)
                }
            }
        }
        Item {
            id: well

            width: column.width
            height: Math.max(40, Math.min(input.contentHeight, 150) + 14)

            Behavior on height { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

            Well {
                id: box
                anchors.fill: parent
                radius: 20
                opacity: boxSpot.hidden ? 0 : 1

                FoldSpot {
                    id: boxSpot
                    anchors.fill: parent
                    key: "box"
                    place: "slot"
                    fill: KodosiTheme.well
                    radius: 20
                }
            }
            Rectangle {
                anchors.fill: parent
                radius: 20
                color: "transparent"
                border.width: 1
                border.color: KodosiTheme.alpha(KodosiTheme.accent, root.focused ? 0.4 : 0)

                Behavior on border.color { ColorAnimation { duration: KodosiTheme.motionHover } }
            }
            TapHandler {
                onTapped: field.forceActiveFocus()
            }
            DosiFace {
                x: 8
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 8
                size: 24
                mood: Models.Dosi.mood === "resting" && root.focused ? "listening" : Models.Dosi.mood
                opacity: faceSpot.hidden ? 0 : 1

                FoldSpot {
                    id: faceSpot
                    anchors.fill: parent
                    key: "face"
                    place: "slot"
                }
            }
            ScrollView {
                id: input

                x: 40
                y: 7
                width: parent.width - 40 - 42 - (renew.visible ? 32 : 0)
                height: parent.height - 14
                opacity: arrive.progress
                transform: Arrive { id: arrive; delay: 350; rise: 0 }
                onVisibleChanged: {
                    if (visible)
                        arrive.play()
                }

                TextArea {
                    id: field

                    placeholderText: qsTr("Ask Dosi")
                    placeholderTextColor: KodosiTheme.inkFaint
                    color: KodosiTheme.ink
                    selectionColor: KodosiTheme.accent
                    selectedTextColor: KodosiTheme.accentInk
                    font.pixelSize: KodosiTheme.fontCallout
                    wrapMode: TextEdit.Wrap
                    topPadding: 4
                    bottomPadding: 4
                    leftPadding: 0
                    rightPadding: 0
                    background: null
                    enabled: Models.Dosi.state !== "unavailable"
                    objectName: "dosi.input"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Ask Dosi")
                    Component.onCompleted: text = Models.Dosi.draft
                    onTextChanged: Models.Dosi.draft = text
                    Keys.onReturnPressed: event => {
                        if (event.modifiers & (Qt.ShiftModifier | Qt.AltModifier)) {
                            event.accepted = false
                            return
                        }
                        Models.Dosi.send()
                    }
                    Keys.onEnterPressed: event => {
                        if (event.modifiers & (Qt.ShiftModifier | Qt.AltModifier)) {
                            event.accepted = false
                            return
                        }
                        Models.Dosi.send()
                    }
                    Keys.onEscapePressed: Models.DesktopState.dosiOpen = false
                }
            }
            DosiNewChat {
                id: renew
                anchors.right: parent.right
                anchors.rightMargin: 38
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                visible: opacity > 0
                opacity: Models.Dosi.empty ? 0 : 1

                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
            }
            Item {
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                width: 28
                height: 28

                AbstractButton {
                    id: stop

                    anchors.fill: parent
                    visible: opacity > 0
                    opacity: root.stops ? 1 : 0
                    scale: root.stops ? (pressed ? 0.88 : 1) : 0.6
                    hoverEnabled: true
                    objectName: "dosi.stop"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Stop")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Stop (Ctrl+Shift+.)")
                    ToolTip.delay: 600
                    onClicked: Models.Dosi.stop()

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack } }

                    background: Bezel {
                        radius: 14
                        quiet: true
                        pressed: stop.pressed
                        fill: KodosiTheme.lifted
                    }
                    contentItem: Item {
                        Rectangle {
                            anchors.centerIn: parent
                            width: 9
                            height: 9
                            radius: 2
                            color: KodosiTheme.ink
                        }
                    }
                }
                AbstractButton {
                    id: send

                    anchors.fill: parent
                    visible: opacity > 0
                    opacity: root.typed ? 1 : 0
                    scale: root.typed ? (pressed ? 0.88 : 1) : 0.4
                    hoverEnabled: true
                    objectName: "dosi.send"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Send")
                    onClicked: Models.Dosi.send()

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack } }

                    background: Bezel {
                        radius: 14
                        pressed: send.pressed
                        fill: send.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                    }
                    contentItem: Item {
                        KIcon {
                            anchors.centerIn: parent
                            width: 14
                            height: 14
                            name: "send"
                            strokeWidth: 2.6
                            color: KodosiTheme.accentInk
                        }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    radius: 14
                    color: KodosiTheme.alpha(KodosiTheme.ink, 0.06)
                    opacity: !root.stops && !root.typed ? 1 : 0
                    visible: opacity > 0
                    Accessible.ignored: true

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                    KIcon {
                        anchors.centerIn: parent
                        width: 13
                        height: 13
                        name: "send"
                        strokeWidth: 2.4
                        color: KodosiTheme.alpha(KodosiTheme.inkFaint, 0.7)
                    }
                }
            }
        }
    }
}
