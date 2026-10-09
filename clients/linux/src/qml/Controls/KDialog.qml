pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Dialog {
    id: root

    property bool destructive: false

    Component.onCompleted: Models.DesktopState.setModalVisible(root, visible && modal)
    onVisibleChanged: Models.DesktopState.setModalVisible(root, visible && modal)
    onModalChanged: Models.DesktopState.setModalVisible(root, visible && modal)
    Component.onDestruction: Models.DesktopState.setModalVisible(root, false)

    modal: true
    padding: 22
    topPadding: 8
    palette.base: KodosiTheme.well
    palette.button: KodosiTheme.raised
    palette.buttonText: KodosiTheme.ink
    palette.highlight: KodosiTheme.accent
    palette.highlightedText: KodosiTheme.accentInk
    palette.text: KodosiTheme.ink
    palette.window: KodosiTheme.lifted
    palette.windowText: KodosiTheme.ink
    parent: Overlay.overlay
    width: Math.min(implicitWidth, 440, parent ? parent.width - 32 : 440)
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0

    Overlay.modal: Rectangle {
        color: KodosiTheme.alpha("#000000", KodosiTheme.isDark ? 0.5 : 0.22)
    }
    background: Raised {
        radius: KodosiTheme.radiusSheet
        fill: KodosiTheme.lifted
        elevation: 3
    }
    header: Item {
        implicitHeight: visible ? 52 : 0
        visible: root.title.length > 0

        PlainLabel {
            anchors.left: parent.left
            anchors.leftMargin: 22
            anchors.right: parent.right
            anchors.rightMargin: 22
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 6
            elide: Text.ElideRight
            font.pixelSize: KodosiTheme.fontHeadline + 2
            font.weight: Font.DemiBold
            text: root.title
        }
    }
    footer: DialogButtonBox {
        visible: count > 0
        padding: 16
        topPadding: 4
        spacing: 8
        alignment: Qt.AlignRight
        background: Item {}
        delegate: KButton {
            variant: DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole
                ? (root.destructive ? KButton.Danger : KButton.Primary)
                : KButton.Ghost
        }
    }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionHover }
        NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: KodosiTheme.motionHover }
    }
}
