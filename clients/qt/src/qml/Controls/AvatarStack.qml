pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Row {
    id: root

    property var userIds: []
    property real size: 24
    property int limit: 4
    property color ring: KodosiTheme.surface

    spacing: -size * 0.28

    Repeater {
        model: root.userIds.slice(0, root.limit)

        PersonAvatar {
            required property string modelData

            name: Identity.personName(modelData)
            key: modelData
            size: root.size
            isSelf: modelData === Models.Account.userId
            ring: root.ring
        }
    }
    Rectangle {
        visible: root.userIds.length > root.limit
        width: root.size
        height: root.size
        radius: width / 2
        color: KodosiTheme.well
        border.width: 2
        border.color: root.ring

        Text {
            anchors.centerIn: parent
            text: "+" + (root.userIds.length - root.limit)
            color: KodosiTheme.inkMuted
            font.pixelSize: Math.round(root.size * 0.38)
            font.weight: Font.DemiBold
        }
    }
}
