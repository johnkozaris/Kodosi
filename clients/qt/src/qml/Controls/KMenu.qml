import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Menu {
    id: root

    cascade: true
    margins: 8
    padding: 6
    overlap: 1
    width: {
        let widest = 0
        for (let index = 0; index < count; ++index) {
            const item = itemAt(index)
            if (item)
                widest = Math.max(widest, item.implicitWidth)
        }
        return Math.max(implicitBackgroundWidth, Math.min(440, widest + leftPadding + rightPadding))
    }

    delegate: KMenuItem {}
    background: Raised {
        implicitWidth: 200
        radius: KodosiTheme.radiusLg
        fill: KodosiTheme.lifted
        elevation: 2
    }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionHover }
        NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic }
    }
}
