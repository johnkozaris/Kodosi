pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var levels: []
    property string current: ""
    property string hovered: ""
    readonly property int level: levels.findIndex(entry => entry.argument === current)
    readonly property string shown: {
        const argument = hovered.length > 0 ? hovered : current
        const entry = levels.find(item => item.argument === argument)
        return entry ? entry.label : qsTr("Model's own level")
    }
    readonly property Item currentItem: segments.count > root.level + 1 ? segments.itemAt(root.level + 1) : null

    readonly property real autoWidth: Math.ceil(autoMetrics.advanceWidth) + 20

    signal picked(string argument)

    implicitWidth: 290
    implicitHeight: 76
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Effort")

    TextMetrics {
        id: autoMetrics
        text: qsTr("Auto")
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.DemiBold
    }
    PlainLabel {
        x: 14
        y: 10
        text: qsTr("Effort")
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    PlainLabel {
        anchors.right: parent.right
        anchors.rightMargin: 14
        y: 9
        text: root.shown
        color: KodosiTheme.inkMuted
        font.pixelSize: KodosiTheme.fontFootnote
        font.weight: Font.Medium
    }
    Item {
        id: track

        x: 14
        y: 34
        width: root.width - 28
        height: 32

        Well {
            anchors.fill: parent
            radius: height / 2
        }
        Raised {
            visible: root.currentItem !== null
            x: root.currentItem ? 3 + root.currentItem.x : 3
            y: 3
            width: root.currentItem ? root.currentItem.width : 0
            height: parent.height - 6
            radius: height / 2

            Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
            Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
        }
        Row {
            x: 3
            y: 3
            height: parent.height - 6

            Repeater {
                id: segments

                model: [{ id: "", argument: "", label: qsTr("Auto") }].concat(root.levels)

                AbstractButton {
                    id: segment

                    required property var modelData
                    required property int index
                    readonly property bool auto: index === 0
                    readonly property bool selected: modelData.argument === root.current

                    width: auto ? root.autoWidth : (track.width - 6 - root.autoWidth) / Math.max(root.levels.length, 1)
                    height: parent.height
                    hoverEnabled: true
                    activeFocusOnTab: true
                    scale: pressed ? 0.92 : 1
                    objectName: "launcher.effort." + (auto ? "default" : modelData.id)
                    Accessible.id: objectName
                    Accessible.name: modelData.label
                    Accessible.role: Accessible.RadioButton
                    Accessible.checked: selected
                    ToolTip.visible: hovered && !auto
                    ToolTip.text: modelData.label
                    ToolTip.delay: 600

                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

                    onHoveredChanged: {
                        if (hovered)
                            root.hovered = modelData.argument.length > 0 ? modelData.argument : "\u0000"
                        else if (root.hovered === (modelData.argument.length > 0 ? modelData.argument : "\u0000"))
                            root.hovered = ""
                    }
                    onClicked: root.picked(modelData.argument)

                    contentItem: Item {
                        PlainLabel {
                            anchors.centerIn: parent
                            visible: segment.auto
                            text: segment.modelData.label
                            color: segment.selected ? KodosiTheme.ink : KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                            font.weight: Font.DemiBold
                        }
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 6
                            visible: !segment.auto
                            width: 6
                            height: 5 + 11 * (segment.index - 1) / Math.max(root.levels.length - 1, 1)
                            radius: 1.5
                            color: segment.index - 1 <= root.level ? KodosiTheme.accent : KodosiTheme.alpha(KodosiTheme.ink, 0.22)

                            Behavior on color {
                                SequentialAnimation {
                                    PauseAnimation { duration: KodosiTheme.reduceMotion ? 0 : Math.max(0, segment.index - 1) * 35 }
                                    ColorAnimation { duration: KodosiTheme.motionSnappy }
                                }
                            }
                        }
                    }
                    background: Rectangle {
                        radius: height / 2
                        color: "transparent"
                        border.width: segment.visualFocus ? 1.5 : 0
                        border.color: KodosiTheme.accentStrong
                    }
                }
            }
        }
    }
}
