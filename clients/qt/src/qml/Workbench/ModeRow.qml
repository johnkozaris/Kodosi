pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Item {
    id: root

    property string title
    property var options: []
    property string current: ""
    property string hovered: ""
    readonly property var shown: options.find(entry => entry.id === (hovered.length > 0 ? hovered : current)) || null
    readonly property int currentIndex: options.findIndex(entry => entry.id === current)
    readonly property Item currentItem: currentIndex >= 0 && segments.count > currentIndex ? segments.itemAt(currentIndex) : null

    signal picked(string id)

    implicitWidth: 300
    implicitHeight: detail.y + detail.implicitHeight + 12
    Accessible.role: Accessible.Grouping
    Accessible.name: title

    PlainLabel {
        x: 14
        y: 10
        text: root.title
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    Item {
        id: track

        x: 14
        y: 32
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
        }
        Row {
            x: 3
            y: 3
            height: parent.height - 6

            Repeater {
                id: segments

                model: root.options

                AbstractButton {
                    id: segment

                    required property var modelData
                    readonly property bool selected: modelData.id === root.current

                    width: (track.width - 6) / Math.max(root.options.length, 1)
                    height: parent.height
                    hoverEnabled: true
                    activeFocusOnTab: true
                    scale: pressed ? 0.95 : 1
                    objectName: root.objectName + "." + modelData.id
                    Accessible.id: objectName
                    Accessible.name: modelData.label
                    Accessible.description: modelData.detail
                    Accessible.role: Accessible.RadioButton
                    Accessible.checked: selected

                    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

                    onHoveredChanged: {
                        if (hovered)
                            root.hovered = modelData.id
                        else if (root.hovered === modelData.id)
                            root.hovered = ""
                    }
                    onClicked: root.picked(modelData.id)

                    contentItem: PlainLabel {
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        text: segment.modelData.label
                        color: segment.selected ? (segment.modelData.caution ? KodosiTheme.caution : KodosiTheme.ink) : KodosiTheme.inkMuted
                        font.pixelSize: KodosiTheme.fontCaption
                        font.weight: Font.DemiBold
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
    PlainLabel {
        id: detail

        x: 14
        y: track.y + track.height + 8
        width: root.width - 28
        text: root.shown ? root.shown.detail : ""
        color: KodosiTheme.inkMuted
        font.pixelSize: KodosiTheme.fontFootnote
        wrapMode: Text.WordWrap
    }
}
