import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic

AbstractButton {
    id: root

    property Look look: KodosiTheme.terminalLook
    property string title: ""
    property bool quiet: false
    property bool loading: false
    property bool open: false
    property alias glyph: glyphSlot.data
    property alias accessory: accessorySlot.data
    property bool accessoryShown: false
    property bool ready: false

    implicitHeight: 26
    implicitWidth: row.implicitWidth + 18
    hoverEnabled: true
    activeFocusOnTab: true
    scale: pressed ? 0.97 : 1
    Accessible.role: Accessible.ButtonMenu
    Accessible.description: title

    Component.onCompleted: ready = true
    onTitleChanged: {
        if (ready && !KodosiTheme.reduceMotion)
            squash.restart()
    }

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    Squash {
        id: squash
        shape: glyphShape
    }

    background: Bezel {
        radius: KodosiTheme.radiusSm
        quiet: true
        look: root.look
        pressed: root.down
        fill: root.open ? KodosiTheme.mix(root.look.accentSoft, root.look.accent, root.hovered ? 0.14 : 0)
            : KodosiTheme.mix(root.look.lifted, root.look.ink, root.hovered ? 0.1 : 0.07)
    }
    contentItem: Item {
        Row {
            id: row

            x: 9
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6

            Item {
                id: glyphSlot
                anchors.verticalCenter: parent.verticalCenter
                width: childrenRect.width
                height: childrenRect.height
                transform: Scale { id: glyphShape; origin.x: glyphSlot.width / 2; origin.y: glyphSlot.height / 2 }
            }
            ShimmerText {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, 130)
                text: root.title
                active: root.loading
                color: root.open ? root.look.accentStrong : root.quiet ? root.look.inkMuted : root.look.ink
                font.pixelSize: KodosiTheme.fontFootnote
                font.weight: Font.DemiBold
                elide: Text.ElideMiddle
            }
            Item {
                id: accessorySlot
                anchors.verticalCenter: parent.verticalCenter
                visible: root.accessoryShown
                width: childrenRect.width
                height: childrenRect.height
            }
            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 9
                height: 9
                name: "chevron-down"
                strokeWidth: 2.8
                color: root.open ? root.look.accentStrong : root.look.inkFaint
                rotation: root.open ? 180 : 0

                Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
            }
        }
    }

    KFocusIndicator {
        active: root.visualFocus
        color: root.look.accent
    }
}
