pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

// The card stays light in both themes, as a printed code does, so that each camera reads its
// dark modules.
Item {
    id: root

    property var rows: []
    readonly property int quietZone: 4

    implicitWidth: 220
    implicitHeight: 220
    Accessible.role: Accessible.Graphic

    Raised {
        anchors.fill: parent
        radius: KodosiTheme.radiusXl
        fill: "#fefbf5"
        dark: false
    }
    Canvas {
        id: modules

        anchors.fill: parent
        visible: root.rows.length > 0

        onPaint: {
            const context = getContext("2d");
            context.clearRect(0, 0, width, height);
            const count = root.rows.length;
            if (count === 0)
                return;
            const module = Math.floor(Math.min(width, height) / (count + 2 * root.quietZone));
            const left = Math.floor((width - module * count) / 2);
            const top = Math.floor((height - module * count) / 2);
            context.fillStyle = "#25160e";
            for (let y = 0; y < count; ++y) {
                const row = root.rows[y];
                for (let x = 0; x < row.length; ++x) {
                    if (row.charAt(x) === "1")
                        context.fillRect(left + x * module, top + y * module, module, module);
                }
            }
        }
    }
    Spinner {
        anchors.centerIn: parent
        size: 18
        visible: root.rows.length === 0
    }

    onRowsChanged: modules.requestPaint()
}
