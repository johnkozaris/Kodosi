import Kodosi 1.0
import QtQuick

DropArea {
    id: root

    property real radius: KodosiTheme.radiusMd
    property bool dashed: true
    property string label: ""

    signal terminalDropped(string sessionId, point at)

    keys: ["application/x-kodosi-terminal"]
    onDropped: drop => {
        const id = drop.getDataAsString("application/x-kodosi-terminal")
        if (id.length === 0)
            return
        drop.accept(Qt.MoveAction)
        root.terminalDropped(id, Qt.point(drop.x, drop.y))
    }

    DropSlot {
        anchors.fill: parent
        shown: root.containsDrag
        radius: root.radius
        dashed: root.dashed
        label: root.label
    }
}
