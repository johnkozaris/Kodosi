import Kodosi 1.0
import QtQuick
import QtQuick.Controls

ScrollView {
    id: root

    ScrollBar.vertical: KScrollBar {
        x: root.width - width
        height: root.availableHeight
    }
    ScrollBar.horizontal: KScrollBar {
        y: root.height - height
        width: root.availableWidth
        policy: ScrollBar.AlwaysOff
    }
}
