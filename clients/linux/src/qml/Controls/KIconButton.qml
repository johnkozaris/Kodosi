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
    scale: pressed && enabled ? 0.9 : 1

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    contentItem: Item {
        KIcon {
            anchors.centerIn: parent
            width: Math.round(root.size * 0.52)
            height: width
            name: root.glyph
            strokeWidth: 1.9
            color: !root.enabled ? KodosiTheme.alpha(root.glyphColor, 0.4)
                : root.active ? (root.onTerminal ? KodosiTheme.terminalAccent : KodosiTheme.accentStrong)
                : root.hovered && root.destructive ? KodosiTheme.danger
                : root.hovered ? root.ink
                : root.glyphColor
        }
    }
    background: Rectangle {
        radius: Math.min(width, height) * 0.32
        color: root.active ? KodosiTheme.alpha(root.onTerminal ? KodosiTheme.terminalAccent : KodosiTheme.accent, 0.18)
            : KodosiTheme.alpha(root.ink, root.hovered && root.enabled ? 0.09 : 0)
        border.width: root.visualFocus ? 1.5 : 0
        border.color: KodosiTheme.accentStrong

        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
    }
}
