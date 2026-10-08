import Kodosi 1.0
import QtQuick
import QtQuick.Controls

MenuItem {
    id: root

    implicitHeight: 36
    leftPadding: checkable ? 34 : 12
    rightPadding: 12

    indicator: KIcon {
        x: 10
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: 14
        implicitHeight: 14
        name: "check"
        visible: root.checkable && root.checked
        color: KodosiTheme.accent
    }

    contentItem: PlainLabel {
        text: root.text
        color: !root.enabled
            ? KodosiTheme.disabled
            : root.highlighted
              ? KodosiTheme.textPrimary
              : KodosiTheme.textSecondary
        font.pixelSize: 11
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        color: root.highlighted
            ? KodosiTheme.surfaceSelected
            : KodosiTheme.surface
        radius: KodosiTheme.radiusSmall
    }
}
