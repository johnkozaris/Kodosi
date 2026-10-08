import Kodosi 1.0
import QtQuick
import QtQuick.Controls

SpinBox {
    id: root

    implicitHeight: KodosiTheme.controlHeight
    editable: true

    contentItem: TextInput {
        z: 2
        text: root.displayText
        color: root.enabled
            ? KodosiTheme.textPrimary
            : KodosiTheme.disabled
        selectionColor: KodosiTheme.accent
        selectedTextColor: KodosiTheme.accentForeground
        font.pixelSize: 12
        horizontalAlignment: Qt.AlignLeft
        verticalAlignment: Qt.AlignVCenter
        leftPadding: 11
        rightPadding: 41
        readOnly: !root.editable
        validator: root.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
    }

    up.indicator: Rectangle {
        x: root.width - width
        width: 30
        height: root.height / 2
        color: root.up.pressed
            ? KodosiTheme.surfaceSelected
            : root.up.hovered
              ? KodosiTheme.surfaceElevated
              : KodosiTheme.surface
        topRightRadius: KodosiTheme.radiusSmall

        KIcon {
            anchors.centerIn: parent
            width: 11
            height: 11
            name: "chevron-up"
            color: KodosiTheme.textSecondary
        }
    }

    down.indicator: Rectangle {
        x: root.width - width
        y: root.height / 2
        width: 30
        height: root.height - y
        color: root.down.pressed
            ? KodosiTheme.surfaceSelected
            : root.down.hovered
              ? KodosiTheme.surfaceElevated
              : KodosiTheme.surface
        bottomRightRadius: KodosiTheme.radiusSmall

        KIcon {
            anchors.centerIn: parent
            width: 11
            height: 11
            name: "chevron-down"
            color: KodosiTheme.textSecondary
        }
    }

    KFocusIndicator { active: root.activeFocus && root.enabled; z: 3 }

    background: Rectangle {
        color: KodosiTheme.input
        radius: KodosiTheme.radiusSmall
    }

    Rectangle {
        anchors.right: parent.right
        anchors.rightMargin: 30
        width: 1
        height: parent.height
        color: KodosiTheme.seam
    }

    Rectangle {
        anchors.right: parent.right
        width: 30
        height: 1
        y: Math.floor(parent.height / 2)
        color: KodosiTheme.seam
    }
}
