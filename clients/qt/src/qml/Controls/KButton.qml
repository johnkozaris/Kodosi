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
    readonly property bool solid: variant === KButton.Primary || variant === KButton.Secondary
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
    opacity: enabled || variant !== KButton.Primary ? 1 : 0.55

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
        Raised {
            anchors.fill: parent
            visible: root.solid
            radius: height / 2
            elevation: root.enabled ? 1 : 0
            fill: root.variant === KButton.Primary
                ? (!root.enabled ? KodosiTheme.mix(KodosiTheme.accent, KodosiTheme.surface, 0.45) : root.hovered ? KodosiTheme.accentHover : KodosiTheme.accent)
                : (root.hovered && root.enabled ? KodosiTheme.lifted : KodosiTheme.raised)
        }
        Rectangle {
            anchors.fill: parent
            visible: !root.solid
            radius: height / 2
            color: root.variant === KButton.Tinted ? KodosiTheme.accentSoft
                : root.variant === KButton.Danger ? KodosiTheme.dangerSoft
                : KodosiTheme.alpha(KodosiTheme.ink, root.hovered && root.enabled ? 0.08 : 0)

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
