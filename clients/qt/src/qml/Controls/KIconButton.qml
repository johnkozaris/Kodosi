import Kodosi 1.0
import QtQuick
import QtQuick.Controls

AbstractButton {
    id: root

    required property string glyph
    property int size: 28
    property bool active: false
    property bool open: false
    property bool destructive: false
    property bool onTerminal: false
    property color glyphColor: onTerminal ? KodosiTheme.terminalInkMuted : KodosiTheme.inkMuted
    readonly property color ink: onTerminal ? KodosiTheme.terminalInk : KodosiTheme.ink
    readonly property bool lit: hovered && enabled
    readonly property real radius: Math.min(KodosiTheme.radiusMd, size * 0.32)

    activeFocusOnTab: true
    hoverEnabled: true
    implicitWidth: size
    implicitHeight: size
    scale: pressed && enabled ? 0.95 : 1
    Accessible.selected: active
    ToolTip.text: Accessible.name
    ToolTip.visible: hovered && !open && ToolTip.text.length > 0
    ToolTip.delay: Tip.delay

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    contentItem: Item {
        KIcon {
            anchors.centerIn: parent
            width: Math.round(root.size * 0.52)
            height: width
            name: root.glyph
            strokeWidth: 1.9
            color: !root.enabled ? (root.onTerminal ? KodosiTheme.mix(KodosiTheme.terminalInkFaint, KodosiTheme.terminal, 0.35) : KodosiTheme.mix(KodosiTheme.inkFaint, KodosiTheme.surface, 0.35))
                : root.lit && root.destructive ? KodosiTheme.danger
                : root.active ? (root.onTerminal ? KodosiTheme.terminalAccentStrong : KodosiTheme.accentStrong)
                : root.lit || root.open ? root.ink
                : root.glyphColor
        }
    }
    background: Item {
        Raised {
            anchors.fill: parent
            radius: root.radius
            fill: root.onTerminal ? KodosiTheme.terminalLifted : KodosiTheme.raised
            dark: root.onTerminal || KodosiTheme.isDark
            opacity: root.active || root.open ? 1 : 0
            visible: opacity > 0

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
        }
        Bezel {
            anchors.fill: parent
            radius: root.radius
            quiet: true
            dark: root.onTerminal || KodosiTheme.isDark
            pressed: root.pressed
            fill: root.lit && root.destructive ? KodosiTheme.dangerSoft
                : root.onTerminal ? KodosiTheme.mix(KodosiTheme.terminalLifted, KodosiTheme.terminalInk, 0.1) : KodosiTheme.buttonFace(true)
            opacity: root.lit && !root.active && !root.open ? 1 : 0
            visible: opacity > 0

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }
        }
        Rectangle {
            anchors.fill: parent
            radius: root.radius
            color: "transparent"
            border.width: root.visualFocus ? 1.5 : 0
            border.color: KodosiTheme.accentStrong
        }
    }
}
