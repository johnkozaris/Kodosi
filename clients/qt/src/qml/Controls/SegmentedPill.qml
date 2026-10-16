pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property var options: []
    property var currentValue
    property bool compact: false
    property bool dock: false
    property string identifier: "segment"
    readonly property int currentIndex: {
        for (let index = 0; index < options.length; ++index) {
            if (options[index].value === currentValue)
                return index
        }
        return -1
    }
    readonly property Item currentItem: currentIndex >= 0 && items.count > currentIndex ? items.itemAt(currentIndex) : null

    signal activated(var value)

    implicitHeight: compact ? 28 : 34
    implicitWidth: row.implicitWidth + 6
    Accessible.role: Accessible.PageTabList

    Well {
        anchors.fill: parent
        radius: height / 2
    }
    Raised {
        visible: root.currentItem !== null
        x: root.currentItem ? row.x + root.currentItem.x : 0
        y: 3
        width: root.currentItem ? root.currentItem.width : 0
        height: root.height - 6
        radius: height / 2

        Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
        Behavior on width { enabled: !root.dock; NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    }
    Row {
        id: row
        x: 3
        y: 3
        height: root.height - 6

        Models.KeyedList {
            id: optionList
            key: "value"
            values: root.options
        }
        Repeater {
            id: items
            model: optionList

            AbstractButton {
                id: option

                required property var modelData
                required property int index
                readonly property bool selected: root.currentIndex === index

                objectName: root.identifier + "." + modelData.value
                Accessible.id: objectName
                Accessible.name: modelData.label
                Accessible.description: root.dock && modelData.count > 0 ? String(modelData.count) : ""
                Accessible.role: Accessible.PageTab
                Accessible.selected: selected
                activeFocusOnTab: true
                hoverEnabled: true
                height: row.height
                width: content.implicitWidth + (root.compact || root.dock ? 22 : 28)
                clip: true
                ToolTip.text: modelData.label
                ToolTip.visible: root.dock && hovered && !selected
                ToolTip.delay: 600

                Behavior on width { enabled: root.dock; NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                contentItem: Item {
                    RowLayout {
                        id: content
                        anchors.verticalCenter: parent.verticalCenter
                        x: root.compact || root.dock ? 11 : 14
                        spacing: 6

                        ProgressRing {
                            visible: option.modelData.progress !== undefined
                            Layout.preferredWidth: 14
                            Layout.preferredHeight: 14
                            progress: option.modelData.progress || 0
                            track: KodosiTheme.alpha(KodosiTheme.ink, 0.16)
                        }
                        KIcon {
                            visible: !!option.modelData.icon && option.modelData.progress === undefined
                            Layout.preferredWidth: 13
                            Layout.preferredHeight: 13
                            name: option.modelData.icon || ""
                            color: root.dock ? (option.selected ? KodosiTheme.accentStrong : KodosiTheme.inkMuted) : label.color
                            strokeWidth: 2
                        }
                        Text {
                            id: label
                            visible: option.selected || !root.dock
                            text: option.modelData.label
                            textFormat: Text.PlainText
                            color: option.selected ? KodosiTheme.ink : option.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                            font.pixelSize: root.compact || root.dock ? KodosiTheme.fontFootnote : KodosiTheme.fontBody
                            font.weight: option.selected && !root.dock ? Font.DemiBold : Font.Medium

                            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                        }
                        RollingText {
                            visible: root.dock && !option.selected && option.modelData.count > 0
                            text: String(option.modelData.count ?? "")
                            color: KodosiTheme.inkFaint
                            font.pixelSize: KodosiTheme.fontFootnote
                            font.weight: Font.Medium
                        }
                    }
                }
                background: Rectangle {
                    radius: height / 2
                    color: "transparent"
                    border.width: option.visualFocus ? 1.5 : 0
                    border.color: KodosiTheme.accentStrong
                }

                BreathingDot {
                    x: option.width - width - 3
                    y: 3
                    size: 6
                    color: KodosiTheme.glowOrange
                    visible: option.modelData.fresh === true && !option.selected
                }

                onClicked: root.activated(modelData.value)
            }
        }
    }
}
