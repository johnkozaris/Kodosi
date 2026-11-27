import Kodosi 1.0
import QtQuick

KMenu {
    id: root

    property var target: null
    property bool titled: false
    property string identifier: ""

    signal mentionChosen(var target)
    signal handOffChosen(var target)

    minimumWidth: 180

    PlainLabel {
        visible: root.titled
        height: visible ? implicitHeight : 0
        leftPadding: 10
        topPadding: 4
        bottomPadding: 2
        text: root.target ? root.target.name : ""
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.Medium
        elide: Text.ElideRight
    }
    KMenuItem {
        text: qsTr("Mention")
        iconName: "at"
        objectName: root.identifier + ".mention"
        onTriggered: root.mentionChosen(root.target)
    }
    KMenuItem {
        text: qsTr("Hand off")
        iconName: "turn-down-right"
        objectName: root.identifier + ".handOff"
        onTriggered: root.handOffChosen(root.target)
    }
}
