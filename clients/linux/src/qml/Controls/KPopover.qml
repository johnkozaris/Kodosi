import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Popup {
    id: root

    Component.onCompleted: Models.DesktopState.setModalVisible(root, visible && modal)
    onVisibleChanged: Models.DesktopState.setModalVisible(root, visible && modal)
    onModalChanged: Models.DesktopState.setModalVisible(root, visible && modal)
    Component.onDestruction: Models.DesktopState.setModalVisible(root, false)

    padding: 0
    palette.window: KodosiTheme.surfaceRaised
    palette.windowText: KodosiTheme.textPrimary
    palette.base: KodosiTheme.input
    palette.alternateBase: KodosiTheme.surfaceElevated
    palette.button: KodosiTheme.surfaceRaised
    palette.buttonText: KodosiTheme.textPrimary
    palette.text: KodosiTheme.textPrimary
    palette.highlight: KodosiTheme.accent
    palette.highlightedText: KodosiTheme.accentForeground
    palette.placeholderText: KodosiTheme.placeholderText
    palette.mid: KodosiTheme.seam

    Overlay.modal: Rectangle {
        color: KodosiTheme.overlayDim
    }

    background: Rectangle {
        color: KodosiTheme.surfaceRaised
        radius: KodosiTheme.radiusLarge
    }
}
