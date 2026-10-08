import Kodosi 1.0
import QtQuick
import QtQuick.Controls

CheckBox {
    id: root

    spacing: 9
    implicitHeight: 28

    indicator: Rectangle {
        implicitWidth: 19
        implicitHeight: 19
        x: root.leftPadding
        y: Math.round((root.height - height) / 2)
        radius: 6
        color: root.checked
            ? KodosiTheme.accent
            : KodosiTheme.input

        KIcon {
            anchors.centerIn: parent
            visible: root.checked
            width: 12
            height: 12
            name: "check"
            strokeWidth: 2.2
            color: KodosiTheme.accentForeground
        }
    }

    KFocusIndicator { active: root.activeFocus && root.enabled }

    contentItem: PlainLabel {
        text: root.text
        color: root.enabled
            ? KodosiTheme.textPrimary
            : KodosiTheme.disabled
        font.pixelSize: 12
        verticalAlignment: Text.AlignVCenter
        leftPadding: root.indicator.width + root.spacing
    }
}
