import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: root

    property string title: ""
    property string message: ""
    property alias art: artSlot.data
    default property alias actions: actionRow.data

    spacing: 14

    Item {
        id: artSlot
        Layout.alignment: Qt.AlignHCenter
        implicitWidth: childrenRect.width
        implicitHeight: childrenRect.height
        visible: children.length > 0
    }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 5

        PlainLabel {
            Layout.fillWidth: true
            text: root.title
            font.pixelSize: KodosiTheme.fontHeadline
            font.weight: Font.DemiBold
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
        PlainLabel {
            Layout.fillWidth: true
            Layout.maximumWidth: 340
            Layout.alignment: Qt.AlignHCenter
            visible: root.message.length > 0
            text: root.message
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
    }
    RowLayout {
        id: actionRow
        Layout.alignment: Qt.AlignHCenter
        Layout.topMargin: 4
        spacing: 8
        visible: children.length > 0
    }
}
