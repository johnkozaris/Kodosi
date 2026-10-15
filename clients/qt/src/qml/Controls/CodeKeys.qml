pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Row {
    id: root

    property string code: ""

    spacing: 6
    Accessible.role: Accessible.StaticText

    Repeater {
        model: root.code.split("")

        Item {
            id: key

            required property string modelData
            readonly property bool separator: modelData === "-" || modelData === " "

            width: separator ? 12 : 40
            height: 52

            Raised { anchors.fill: parent; visible: !key.separator; radius: KodosiTheme.radiusMd }
            Text {
                anchors.centerIn: parent
                text: key.modelData
                color: key.separator ? KodosiTheme.inkFaint : KodosiTheme.ink
                font.family: "monospace"
                font.pixelSize: 24
                font.weight: Font.DemiBold
            }
        }
    }
}
