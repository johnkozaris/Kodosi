pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property string key: ""
    property real size: 28
    readonly property color tint: Identity.roomTint(key)
    readonly property int seed: Models.Appearance.stableBits(key)
    readonly property real gap: size * 0.07
    readonly property real cell: (size - size * 0.4 - gap) / 2

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Rectangle {
        anchors.fill: parent
        radius: root.size * 0.3
        gradient: Gradient {
            GradientStop { position: 0; color: KodosiTheme.mix(root.tint, "#ffffff", 0.16) }
            GradientStop { position: 0.5; color: KodosiTheme.mix(root.tint, "#000000", 0.08) }
            GradientStop { position: 1; color: KodosiTheme.mix(root.tint, "#000000", 0.34) }
        }
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.18)
    }
    Grid {
        anchors.centerIn: parent
        columns: 2
        spacing: root.gap

        Repeater {
            model: 4

            Rectangle {
                required property int index

                width: root.cell
                height: root.cell
                radius: root.cell * 0.28
                color: "#ffffff"
                opacity: ((root.seed >> (index + 3)) & 1) === 1 || index === root.seed % 4 ? 0.92 : 0.28
            }
        }
    }
}
