pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

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

        Repeater {
            id: items
            model: root.options

            AbstractButton {
                id: option

                required property var modelData
                required property int index
                readonly property bool selected: root.currentIndex === index

                objectName: root.identifier + "." + modelData.value
                Accessible.id: objectName
                Accessible.name: modelData.label
                Accessible.role: Accessible.PageTab
                Accessible.selected: selected
                activeFocusOnTab: true
                hoverEnabled: true
                height: row.height
                width: content.implicitWidth + (root.compact ? 22 : 28)
                clip: true

                Behavior on width { enabled: root.dock; NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                contentItem: Item {
                    RowLayout {
                        id: content
                        anchors.verticalCenter: parent.verticalCenter
                        x: root.compact ? 11 : 14
                        spacing: 6

                        ProgressRing {
                            visible: option.modelData.progress !== undefined
                            Layout.preferredWidth: 13
                            Layout.preferredHeight: 13
                            progress: option.modelData.progress || 0
                            track: KodosiTheme.alpha(label.color, 0.35)
                        }
                        KIcon {
                            visible: !!option.modelData.icon && option.modelData.progress === undefined
                            Layout.preferredWidth: 13
                            Layout.preferredHeight: 13
                            name: option.modelData.icon || ""
                            color: label.color
                            strokeWidth: 2
                        }
                        Text {
                            id: label
                            visible: option.selected || !root.dock
                            text: option.modelData.label
                            textFormat: Text.PlainText
                            color: option.selected ? KodosiTheme.ink : option.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                            font.pixelSize: root.compact ? KodosiTheme.fontFootnote : KodosiTheme.fontBody
                            font.weight: option.selected ? Font.DemiBold : Font.Medium

                            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                        }
                        Text {
                            visible: root.dock && !option.selected && option.modelData.count > 0
                            text: option.modelData.count ?? ""
                            color: label.color
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

                onClicked: root.activated(modelData.value)
            }
        }
    }
}
