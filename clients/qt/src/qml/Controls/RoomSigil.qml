pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Shapes
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

    component Cell: ShapePath {
        id: cell

        required property int index
        readonly property real left: root.size * 0.2 + (index % 2) * (root.cell + root.gap)
        readonly property real top: root.size * 0.2 + Math.floor(index / 2) * (root.cell + root.gap)

        strokeColor: "transparent"
        fillColor: Qt.rgba(1, 1, 1, ((root.seed >> (index + 3)) & 1) === 1 || index === root.seed % 4 ? 0.92 : 0.28)

        PathSvg { path: KodosiTheme.squircle(cell.left, cell.top, root.cell, root.cell, root.cell * 0.28) }
    }

    Squircle {
        anchors.fill: parent
        lightColor: KodosiTheme.mix(root.tint, "#ffffff", 0.16)
        baseColor: KodosiTheme.mix(root.tint, "#000000", 0.08)
        darkColor: KodosiTheme.mix(root.tint, "#000000", 0.34)
        rimLight: 0.42
        rimShade: 0.2
        shadow: KodosiTheme.isDark ? 0.45 : 0.2

        Cell { index: 0 }
        Cell { index: 1 }
        Cell { index: 2 }
        Cell { index: 3 }
    }
}
