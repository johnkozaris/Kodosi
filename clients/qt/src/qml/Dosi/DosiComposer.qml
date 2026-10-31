pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
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

    implicitHeight: column.implicitHeight + 16
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
        x: 10
        y: 6
        width: parent.width - 20
        spacing: 6

        add: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade }
            NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
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

                Bezel {
                    anchors.fill: parent
                    radius: height / 2
                    quiet: true
                    fill: KodosiTheme.raised
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
            height: Math.min(150, Math.max(40, field.implicitHeight + 10))

            Behavior on height { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

            Raised {
                anchors.fill: parent
                radius: 20
                fill: KodosiTheme.lifted
                elevation: root.focused ? 2 : 1
                opacity: boxSpot.hidden ? 0 : 1

                FoldSpot {
                    id: boxSpot
                    anchors.fill: parent
                    key: "box"
                    place: "slot"
                    fill: KodosiTheme.lifted
                    radius: 20
                }
            }
            Rectangle {
                anchors.fill: parent
                radius: 20
                color: "transparent"
                border.width: 1.5
                border.color: KodosiTheme.mix(KodosiTheme.accent, KodosiTheme.lifted, 0.5)
                opacity: root.focused ? 1 : 0

                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }
            }
            TapHandler {
                onTapped: field.forceActiveFocus()
            }
            RowLayout {
                anchors.fill: parent
                anchors.margins: 5
                spacing: 4

                Item {
                    Layout.alignment: Qt.AlignBottom
                    implicitWidth: 30
                    implicitHeight: 30

                    DosiFace {
                        id: face

                        readonly property point look: Fold.look(face, !Models.Dosi.busy && !root.focused)

                        anchors.centerIn: parent
                        size: 20
                        mood: Models.Dosi.mood === "resting" && root.focused ? "listening" : Models.Dosi.mood
                        lookX: look.x
                        lookY: look.y
                        opacity: faceSpot.hidden ? 0 : 1

                        FoldSpot {
                            id: faceSpot
                            anchors.fill: parent
                            key: "face"
                            place: "slot"
                        }
                    }
                }
                ScrollView {
                    id: input

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    opacity: arrive.progress
                    transform: Arrive { id: arrive; delay: 120; rise: 0 }
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
                        topPadding: 6
                        bottomPadding: 6
                        leftPadding: 2
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
                Item {
                    Layout.alignment: Qt.AlignBottom
                    implicitWidth: 30
                    implicitHeight: 30

                    AbstractButton {
                        id: stop

                        anchors.fill: parent
                        visible: opacity > 0
                        opacity: root.stops ? 1 : 0
                        scale: root.stops ? (pressed ? 0.95 : 1) : 0.9
                        hoverEnabled: true
                        objectName: "dosi.stop"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Stop")
                        ToolTip.visible: hovered
                        ToolTip.text: Models.Shortcuts.hints.stopDosi ? qsTr("Stop (%1)").arg(Models.Shortcuts.hints.stopDosi) : qsTr("Stop")
                        ToolTip.delay: Tip.delay
                        onClicked: Models.Dosi.stop()

                        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }

                        background: Bezel {
                            radius: 15
                            quiet: true
                            pressed: stop.pressed
                            fill: KodosiTheme.buttonFace(stop.hovered)
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
                        visible: !root.stops
                        enabled: root.typed
                        scale: pressed ? 0.95 : 1
                        hoverEnabled: true
                        objectName: "dosi.send"
                        Accessible.id: objectName
                        Accessible.name: qsTr("Send")
                        ToolTip.visible: hovered && root.typed
                        ToolTip.text: qsTr("Send")
                        ToolTip.delay: Tip.delay
                        onClicked: Models.Dosi.send()

                        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                        background: Item {
                            Rectangle {
                                anchors.fill: parent
                                radius: 15
                                color: KodosiTheme.mix(KodosiTheme.lifted, KodosiTheme.ink, 0.08)
                            }
                            Bezel {
                                anchors.fill: parent
                                radius: 15
                                pressed: send.pressed
                                fill: send.hovered ? KodosiTheme.accentHover : KodosiTheme.accent
                                opacity: root.typed ? 1 : 0

                                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
                            }
                        }
                        contentItem: Item {
                            KIcon {
                                anchors.centerIn: parent
                                width: 14
                                height: 14
                                name: "send"
                                strokeWidth: 2.6
                                color: root.typed ? KodosiTheme.accentInk : KodosiTheme.inkFaint
                            }
                        }
                    }
                }
            }
        }
    }
}
