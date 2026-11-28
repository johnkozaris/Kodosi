pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Row {
    id: root

    property int count: 0
    property real size: 26
    property Look look: KodosiTheme.look

    spacing: Math.round(size * 0.22)
    visible: count > 0

    Repeater {
        model: root.count

        Rectangle {
            width: root.size
            height: root.size
            radius: Math.round(root.size * 0.28)
            color: KodosiTheme.mix(root.look.accentSoft, root.look.ink, 0.1)
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("Picture")

            KIcon {
                anchors.centerIn: parent
                width: Math.round(root.size * 0.56)
                height: width
                name: "picture"
                strokeWidth: 1.9
                color: root.look.inkMuted
            }
        }
    }
}
