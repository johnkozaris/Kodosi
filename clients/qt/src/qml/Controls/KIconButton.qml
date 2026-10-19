import Kodosi 1.0
import QtQuick
import QtQuick.Controls

AbstractButton {
    id: root

    required property string glyph
    property int size: 28
    property bool active: false
    property bool destructive: false
    property bool onTerminal: false
    property color glyphColor: onTerminal ? KodosiTheme.terminalInkMuted : KodosiTheme.inkMuted
    readonly property color ink: onTerminal ? KodosiTheme.terminalInk : KodosiTheme.ink

    activeFocusOnTab: true
    hoverEnabled: true
    implicitWidth: size
    implicitHeight: size
    opacity: enabled ? 1 : 0.35
    scale: pressed && enabled ? 0.95 : 1
    ToolTip.text: Accessible.name
    ToolTip.visible: hovered && ToolTip.text.length > 0
    ToolTip.delay: Tip.delay

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    contentItem: Item {
        KIcon {
            anchors.centerIn: parent
            width: Math.round(root.size * 0.52)
            height: width
            name: root.glyph
            strokeWidth: 1.9
            color: root.hovered && root.enabled && root.destructive ? KodosiTheme.danger
                : root.active ? (root.onTerminal ? KodosiTheme.terminalAccent : KodosiTheme.accentStrong)
                : root.hovered && root.enabled ? root.ink
                : root.glyphColor
        }
    }
    background: Item {
        readonly property bool lit: root.hovered && root.enabled

        Bezel {
            anchors.fill: parent
            radius: Math.min(KodosiTheme.radiusMd, root.size * 0.32)
            quiet: true
            pressed: root.pressed
            fill: parent.lit && root.destructive ? KodosiTheme.dangerSoft
                : root.active ? (root.onTerminal ? KodosiTheme.terminalAccentSoft : KodosiTheme.accentSoft)
                : root.onTerminal ? KodosiTheme.mix(KodosiTheme.terminalLifted, KodosiTheme.terminalInk, 0.1) : KodosiTheme.buttonFace(true)
            opacity: parent.lit || root.active ? 1 : 0

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }
        }
        Rectangle {
            anchors.fill: parent
            radius: Math.min(KodosiTheme.radiusMd, root.size * 0.32)
            color: "transparent"
            border.width: root.visualFocus ? 1.5 : 0
            border.color: KodosiTheme.accentStrong
        }
    }
}
