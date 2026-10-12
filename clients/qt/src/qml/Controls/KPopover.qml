import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Popup {
    id: root

    property real radius: KodosiTheme.radiusXl
    property int elevation: 2

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
        color: KodosiTheme.alpha("#000000", KodosiTheme.isDark ? 0.5 : 0.22)

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
    }
    background: Raised {
        radius: root.radius
        fill: KodosiTheme.lifted
        elevation: root.elevation
    }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionHover }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: KodosiTheme.motionHover }
    }
}
