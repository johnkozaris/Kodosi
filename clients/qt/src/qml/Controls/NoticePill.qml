import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

Item {
    id: root

    property string text
    property var sign: null
    property string actionText
    property string identifier

    signal actionClicked
    signal dismissed

    implicitWidth: row.implicitWidth + 28
    implicitHeight: Math.max(40, label.implicitHeight + 20)
    objectName: identifier

    Raised {
        anchors.fill: parent
        radius: Math.min(20, height / 2)
        fill: KodosiTheme.lifted
        elevation: 3
    }
    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: root.sign ? 12 : 16
        anchors.rightMargin: 8
        spacing: 10

        Rectangle { visible: !root.sign; Layout.preferredWidth: 7; Layout.preferredHeight: 7; radius: 3.5; color: KodosiTheme.caution }
        StatusSign { session: root.sign }
        PlainLabel {
            id: label
            Layout.fillWidth: true
            Layout.maximumWidth: 520
            text: root.text
            wrapMode: Text.WordWrap
            maximumLineCount: 3
            elide: Text.ElideRight
            objectName: root.identifier + ".text"
            Accessible.id: objectName
        }
        KButton {
            Accessible.id: objectName
            compact: true
            visible: root.actionText.length > 0
            text: root.actionText
            objectName: root.identifier + ".action"

            onClicked: root.actionClicked()
        }
        KIconButton {
            Accessible.id: objectName
            Accessible.name: qsTr("Dismiss")
            glyph: "close"
            size: 26
            objectName: root.identifier + ".dismiss"

            onClicked: root.dismissed()
        }
    }
}
