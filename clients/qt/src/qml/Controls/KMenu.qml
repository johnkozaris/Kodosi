import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic

Menu {
    id: root

    property Component mark: null
    property string hint: ""
    property bool available: true
    property real minimumWidth: 200
    readonly property real widestItem: {
        let widest = 0
        for (let index = 0; index < count; ++index)
            widest = Math.max(widest, itemAt(index)?.implicitWidth ?? 0)
        return widest
    }

    function popupFor(item: Item, position: point): void {
        // Keyboard context menus have no cursor position on Windows, so anchor below the item.
        popup(item.mapToItem(parent, item.contains(position) ? position : Qt.point(0, item.height + 4)))
    }

    width: Math.min(320, Math.max(minimumWidth, widestItem + leftPadding + rightPadding))
    margins: 8
    padding: 6
    overlap: 1
    delegate: KMenuItem {}

    background: Raised {
        implicitWidth: 200
        radius: KodosiTheme.radiusLg
        fill: KodosiTheme.lifted
        elevation: 2
    }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic }
    }
}
