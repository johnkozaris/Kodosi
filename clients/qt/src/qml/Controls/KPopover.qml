pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Popup {
    id: root

    property real radius: KodosiTheme.radiusXl
    property int elevation: 2
    property color fill: KodosiTheme.lifted
    property bool instant: false

    Component.onCompleted: Models.DesktopState.setModalVisible(root, visible && modal)
    onVisibleChanged: Models.DesktopState.setModalVisible(root, visible && modal)
    onModalChanged: Models.DesktopState.setModalVisible(root, visible && modal)
    Component.onDestruction: Models.DesktopState.setModalVisible(root, false)

    padding: 0
    palette.window: KodosiTheme.lifted
    palette.windowText: KodosiTheme.ink
    palette.base: KodosiTheme.well
    palette.button: KodosiTheme.raised
    palette.buttonText: KodosiTheme.ink
    palette.text: KodosiTheme.ink
    palette.highlight: KodosiTheme.accent
    palette.highlightedText: KodosiTheme.accentInk
    palette.placeholderText: KodosiTheme.inkFaint

    Overlay.modal: Rectangle {
        color: KodosiTheme.alpha(KodosiTheme.shadow, KodosiTheme.isDark ? 0.55 : 0.22)

        Behavior on opacity {
            enabled: !root.instant
            NumberAnimation { duration: KodosiTheme.motionFade }
        }
    }
    background: Raised {
        radius: root.radius
        fill: root.fill
        elevation: root.elevation
    }
    enter: root.instant ? null : opening
    exit: root.instant ? null : closing

    readonly property Transition opening: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
    }
    readonly property Transition closing: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: KodosiTheme.motionHover; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
    }
}
