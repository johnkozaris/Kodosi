import Kodosi 1.0
import QtQuick
import QtQuick.Controls

// The small round key that takes a file off the canvas.
AbstractButton {
    id: root

    property real size: 16
    property bool backed: false

    width: size
    height: size
    hoverEnabled: true
    scale: pressed ? 0.88 : 1
    Accessible.id: objectName
    Accessible.name: qsTr("Remove from canvas")
    ToolTip.text: Accessible.name
    ToolTip.visible: hovered
    ToolTip.delay: Tip.delay

    contentItem: KIcon {
        name: "close"
        strokeWidth: 3
        color: root.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
    }
    background: Rectangle {
        radius: width / 2
        color: root.backed ? Qt.tint(KodosiTheme.sheet, KodosiTheme.alpha(KodosiTheme.ink, root.hovered ? 0.14 : 0.08))
            : KodosiTheme.alpha(KodosiTheme.ink, root.hovered ? 0.14 : 0.08)
    }
    padding: size * 0.25
}
