import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

Item {
    id: root

    property string title: ""
    property string subtitle: ""
    property string iconName: ""
    property color tint: KodosiTheme.accent
    property alias leading: leadingSlot.data
    default property alias trailing: trailingRow.data

    width: parent ? parent.width : implicitWidth
    implicitHeight: Math.max(48, text.implicitHeight + 18)

    Rectangle {
        visible: root.Positioner.index > 0
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        height: 1
        color: KodosiTheme.alpha(KodosiTheme.hairline, 0.6)
    }
    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: 12

        IconTile {
            visible: root.iconName.length > 0
            iconName: root.iconName
            tint: root.tint
            size: 26
        }
        Item {
            id: leadingSlot
            visible: children.length > 0
            implicitWidth: childrenRect.width
            implicitHeight: childrenRect.height
        }
        ColumnLayout {
            id: text
            Layout.fillWidth: true
            spacing: 2

            PlainLabel {
                Layout.fillWidth: true
                text: root.title
                font.weight: Font.Medium
                elide: Text.ElideRight
            }
            PlainLabel {
                Layout.fillWidth: true
                visible: root.subtitle.length > 0
                text: root.subtitle
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideMiddle
            }
        }
        RowLayout {
            id: trailingRow
            spacing: 8
        }
    }
}
