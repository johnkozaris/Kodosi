import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: root

    property string title: ""
    property string footer: ""
    default property alias rows: column.data

    spacing: 8

    PlainLabel {
        Layout.leftMargin: 4
        visible: root.title.length > 0
        text: root.title
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
    }
    Item {
        Layout.fillWidth: true
        implicitHeight: column.implicitHeight

        Raised {
            anchors.fill: parent
            radius: KodosiTheme.radiusLg
        }
        Column {
            id: column
            width: parent.width
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        Layout.leftMargin: 4
        visible: root.footer.length > 0
        text: root.footer
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        wrapMode: Text.WordWrap
    }
}
