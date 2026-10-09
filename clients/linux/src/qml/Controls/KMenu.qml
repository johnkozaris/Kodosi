import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Menu {
    id: root

    margins: 8
    padding: 6
    overlap: 1

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
