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
    readonly property color accent: onTerminal ? KodosiTheme.terminalAccent : KodosiTheme.accentStrong

    activeFocusOnTab: true
    hoverEnabled: true
    implicitWidth: size
    implicitHeight: size
    scale: pressed && enabled ? 0.94 : 1
    opacity: enabled ? 1 : 0.35

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

    contentItem: Item {
        KIcon {
            anchors.centerIn: parent
            width: Math.round(root.size * 0.52)
            height: width
            name: root.glyph
            strokeWidth: 1.9
            color: root.hovered && root.destructive ? KodosiTheme.danger
                : root.active ? root.accent
                : root.hovered ? root.ink
                : root.glyphColor

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
        }
    }
    background: Rectangle {
        radius: Math.min(width, height) / 2
        color: root.hovered && root.destructive ? KodosiTheme.alpha(KodosiTheme.danger, 0.14)
            : root.active ? (root.onTerminal ? KodosiTheme.alpha(KodosiTheme.terminalAccent, 0.18) : KodosiTheme.accentSoft)
            : KodosiTheme.alpha(root.ink, root.hovered ? 0.09 : 0)
        border.width: root.visualFocus ? 1.5 : 0
        border.color: KodosiTheme.accentStrong

        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
    }
}
