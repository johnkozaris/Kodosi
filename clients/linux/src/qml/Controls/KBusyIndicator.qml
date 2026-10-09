import Kodosi 1.0
import QtQuick

Item {
    id: root

    property bool running: true

    implicitWidth: 12
    implicitHeight: 22
    visible: running

    CursorBlock {
        anchors.fill: parent
        blinks: root.running
    }
}
