import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Button {
    id: root

    enum Variant {
        Secondary,
        Primary,
        Quiet,
        Directional
    }

    property int variant: KButton.Secondary
    property string iconName: ""
    property bool iconTrailing: variant === KButton.Directional
    property bool compact: false
    property color iconColor: label.color
    activeFocusOnTab: true

    implicitHeight: compact
        ? KodosiTheme.compactControlHeight
        : KodosiTheme.controlHeight
    implicitWidth: Math.max(
        compact ? 34 : 54,
        contentRow.implicitWidth + leftPadding + rightPadding)
    leftPadding: compact ? 9 : 12
    rightPadding: compact ? 9 : 12
    topPadding: 6
    bottomPadding: 6
    hoverEnabled: true

    readonly property bool filled: variant === KButton.Primary
        || variant === KButton.Directional
    readonly property bool selected: checkable && checked

    contentItem: RowLayout {
        id: contentRow
        spacing: root.iconName.length > 0 ? 7 : 0

        Item {
            Layout.fillWidth: true
        }

        KIcon {
            visible: root.iconName.length > 0 && !root.iconTrailing
            Layout.preferredWidth: 15
            Layout.preferredHeight: 15
            name: root.iconName
            color: root.iconColor
        }

        PlainLabel {
            id: label
            text: root.text
            color: !root.enabled
                ? KodosiTheme.disabled
                : root.selected
                  ? KodosiTheme.accent
                  : root.filled
                  ? KodosiTheme.accentForeground
                  : root.variant === KButton.Quiet
                    ? (root.hovered
                       ? KodosiTheme.textPrimary
                       : KodosiTheme.textSecondary)
                    : KodosiTheme.textPrimary
            font.pixelSize: KodosiTheme.fontBody
            font.weight: Font.Medium
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }

        KIcon {
            visible: root.iconName.length > 0 && root.iconTrailing
            Layout.preferredWidth: 14
            Layout.preferredHeight: 14
            name: root.iconName
            color: root.iconColor
        }
        Item {
            Layout.fillWidth: true
        }
    }

    background: Rectangle {
        radius: KodosiTheme.radiusSmall
        color: {
            if (!root.enabled)
                return KodosiTheme.surfaceRaised
            if (root.selected)
                return root.pressed
                    ? KodosiTheme.surfaceElevated
                    : KodosiTheme.surfaceSelected
            if (root.filled) {
                if (root.pressed)
                    return KodosiTheme.accentPressed
                if (root.hovered)
                    return KodosiTheme.accentHover
                return KodosiTheme.accent
            }
            if (root.pressed)
                return KodosiTheme.surfaceSelected
            if (root.hovered)
                return KodosiTheme.surfaceElevated
            return root.variant === KButton.Quiet
                ? KodosiTheme.surface
                : KodosiTheme.surfaceRaised
        }

        Behavior on color {
            ColorAnimation { duration: KodosiTheme.motionFast }
        }
    }

    KFocusIndicator {
        objectName: "control.keyboardFocus"
        Accessible.id: objectName
        active: root.activeFocus && root.enabled
        color: root.filled ? KodosiTheme.accentForeground : KodosiTheme.focusRing
    }

    transform: Translate { y: root.pressed && root.enabled ? 1 : 0 }
}
