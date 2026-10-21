import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Button {
    id: root

    enum Variant {
        Secondary,
        Primary,
        Ghost,
        Tinted,
        Danger
    }

    property int variant: KButton.Secondary
    property string iconName: ""
    property bool iconTrailing: false
    property bool compact: false
    property bool large: false
    property bool working: false
    readonly property real radius: compact ? KodosiTheme.radiusSm : large ? KodosiTheme.radiusTile : KodosiTheme.radiusMd
    readonly property bool lit: hovered && enabled
    readonly property color foreground: !enabled ? KodosiTheme.inkFaint
        : variant === KButton.Primary ? KodosiTheme.accentInk
        : variant === KButton.Tinted ? KodosiTheme.accentStrong
        : variant === KButton.Danger ? KodosiTheme.danger
        : variant === KButton.Ghost && !hovered ? KodosiTheme.inkMuted
        : KodosiTheme.ink

    activeFocusOnTab: true
    hoverEnabled: true
    implicitHeight: compact ? 26 : large ? 40 : KodosiTheme.controlHeight
    implicitWidth: Math.max(implicitHeight, contentRow.implicitWidth + leftPadding + rightPadding)
    leftPadding: compact ? 11 : large ? 20 : 14
    rightPadding: leftPadding
    topPadding: 0
    bottomPadding: 0
    scale: pressed && enabled ? 0.97 : 1

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    contentItem: RowLayout {
        id: contentRow
        spacing: 7

        Item { Layout.fillWidth: true }
        KIcon {
            visible: root.iconName.length > 0 && !root.iconTrailing
            Layout.preferredWidth: root.compact ? 13 : 15
            Layout.preferredHeight: root.compact ? 13 : 15
            name: root.iconName
            color: root.foreground
            strokeWidth: 2
        }
        ShimmerText {
            text: root.text
            visible: root.text.length > 0
            active: root.working
            color: root.foreground
            font.pixelSize: root.compact ? KodosiTheme.fontFootnote : root.large ? KodosiTheme.fontCallout : KodosiTheme.fontBody
            font.weight: Font.DemiBold
        }
        KIcon {
            visible: root.iconName.length > 0 && root.iconTrailing
            Layout.preferredWidth: 13
            Layout.preferredHeight: 13
            name: root.iconName
            color: root.foreground
            strokeWidth: 2
        }
        Item { Layout.fillWidth: true }
    }

    background: Item {
        Bezel {
            anchors.fill: parent
            visible: root.variant !== KButton.Ghost
            radius: root.radius
            quiet: root.variant !== KButton.Primary || !root.enabled
            pressed: root.pressed && root.enabled
            fill: !root.enabled ? KodosiTheme.buttonFace(false)
                : root.variant === KButton.Primary ? (root.lit ? KodosiTheme.accentHover : KodosiTheme.accent)
                : root.variant === KButton.Tinted ? KodosiTheme.mix(KodosiTheme.accentSoft, KodosiTheme.accent, root.lit ? 0.14 : 0)
                : root.variant === KButton.Danger ? KodosiTheme.mix(KodosiTheme.dangerSoft, KodosiTheme.danger, root.lit ? 0.14 : 0)
                : KodosiTheme.buttonFace(root.lit)
        }
        Rectangle {
            anchors.fill: parent
            visible: root.variant === KButton.Ghost
            radius: root.radius
            color: KodosiTheme.alpha(KodosiTheme.ink, !root.enabled ? 0 : root.pressed ? 0.13 : root.hovered ? 0.08 : 0)

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
        }
    }

    KFocusIndicator {
        objectName: "control.keyboardFocus"
        Accessible.id: objectName
        active: root.visualFocus && root.enabled
        color: root.variant === KButton.Primary ? KodosiTheme.accentInk : KodosiTheme.accentStrong
    }
}
