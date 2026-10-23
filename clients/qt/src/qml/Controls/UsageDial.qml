import Kodosi 1.0
import QtQuick

// One limit as a small dial: the ring with the used percent inside it.
Item {
    id: root

    property var limit: null
    property real size: 26
    readonly property string level: limit ? limit.level : ""

    implicitWidth: size
    implicitHeight: size
    Accessible.role: Accessible.StaticText
    Accessible.name: limit ? limit.spoken : qsTr("Not read yet")
    transform: Scale { id: dialScale; origin.x: root.width / 2; origin.y: root.height / 2 }

    onLevelChanged: {
        if (!KodosiTheme.reduceMotion && level.length > 0)
            squash.restart()
    }

    Squash {
        id: squash
        shape: dialScale
    }
    UsageRing {
        anchors.fill: parent
        limit: root.limit
        lineWidth: Math.max(1.75, root.size * 0.085)
    }
    RollingText {
        anchors.centerIn: parent
        visible: root.limit !== null
        text: root.limit ? String(root.limit.used) : ""
        color: root.level === "reached" ? KodosiTheme.danger : root.level === "near" ? KodosiTheme.caution : KodosiTheme.ink
        font.pixelSize: Math.round(root.size * 0.37)
        font.weight: Font.DemiBold
    }
}
