pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    property var items: []
    property int currentIndex: 0
    property string identifier: "dock"
    readonly property Item currentItem: entries.count > currentIndex ? entries.itemAt(currentIndex) : null

    signal activated(int index)

    implicitHeight: 34
    implicitWidth: row.width + 6
    Accessible.role: Accessible.PageTabList

    Well {
        anchors.fill: parent
        radius: height / 2
    }
    Raised {
        x: root.currentItem ? row.x + root.currentItem.x : 3
        y: 3
        width: root.currentItem ? root.currentItem.width : 0
        height: root.height - 6
        radius: height / 2

        Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
    }
    Row {
        id: row
        x: 3
        y: 3
        height: root.height - 6

        Repeater {
            id: entries
            model: root.items

            AbstractButton {
                id: entry

                required property var modelData
                required property int index
                readonly property bool selected: root.currentIndex === index
                readonly property color tone: selected || hovered ? KodosiTheme.ink : KodosiTheme.inkMuted

                objectName: root.identifier + "." + index
                Accessible.id: objectName
                Accessible.name: modelData.label
                Accessible.role: Accessible.PageTab
                Accessible.selected: selected
                activeFocusOnTab: true
                hoverEnabled: true
                height: row.height
                width: content.implicitWidth + 24
                clip: true

                Behavior on width { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                contentItem: Item {
                    RowLayout {
                        id: content
                        anchors.verticalCenter: parent.verticalCenter
                        x: 12
                        spacing: 6

                        ProgressRing {
                            visible: entry.modelData.progress !== undefined
                            Layout.preferredWidth: 14
                            Layout.preferredHeight: 14
                            progress: entry.modelData.progress || 0
                            track: KodosiTheme.alpha(entry.tone, 0.35)
                        }
                        KIcon {
                            visible: entry.modelData.progress === undefined
                            Layout.preferredWidth: 14
                            Layout.preferredHeight: 14
                            name: entry.modelData.icon
                            color: entry.tone
                            strokeWidth: 1.9
                        }
                        Text {
                            visible: entry.selected
                            text: entry.modelData.label
                            textFormat: Text.PlainText
                            color: KodosiTheme.ink
                            font.pixelSize: KodosiTheme.fontBody
                            font.weight: Font.DemiBold
                        }
                        Text {
                            visible: !entry.selected && entry.modelData.count > 0
                            text: entry.modelData.count
                            color: entry.tone
                            font.pixelSize: KodosiTheme.fontFootnote
                            font.weight: Font.Medium
                        }
                        BreathingDot {
                            visible: !entry.selected && !!entry.modelData.fresh
                            size: 5
                        }
                    }
                }
                background: Rectangle {
                    radius: height / 2
                    color: "transparent"
                    border.width: entry.visualFocus ? 1.5 : 0
                    border.color: KodosiTheme.accentStrong
                }

                onClicked: root.activated(index)
            }
        }
    }
}
