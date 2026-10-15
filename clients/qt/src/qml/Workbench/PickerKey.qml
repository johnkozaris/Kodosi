import Kodosi 1.0
import QtQuick
import QtQuick.Controls

AbstractButton {
    id: root

    property string title: ""
    property bool quiet: false
    property bool loading: false
    property bool open: false
    property alias glyph: glyphSlot.data
    property alias accessory: accessorySlot.data
    property bool ready: false

    implicitHeight: KodosiTheme.controlHeight
    implicitWidth: row.implicitWidth + 22
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

    SequentialAnimation {
        id: squash
        NumberAnimation { target: glyphSlot; property: "scale"; to: 0.78; duration: 80; easing.type: Easing.OutQuad }
        NumberAnimation { target: glyphSlot; property: "scale"; to: 1; duration: KodosiTheme.motionSoft; easing.type: Easing.OutBack; easing.overshoot: 2.4 }
    }

    background: Bezel {
        radius: KodosiTheme.radiusMd
        quiet: true
        pressed: root.down
        fill: root.open ? KodosiTheme.mix(KodosiTheme.accentSoft, KodosiTheme.accent, root.hovered ? 0.14 : 0)
            : KodosiTheme.mix(KodosiTheme.lifted, KodosiTheme.ink, (KodosiTheme.isDark ? 0.07 : 0) + (root.hovered ? 0.03 : 0))
    }
    contentItem: Item {
        Row {
            id: row

            x: 11
            anchors.verticalCenter: parent.verticalCenter
            spacing: 7

            Item {
                id: glyphSlot
                anchors.verticalCenter: parent.verticalCenter
                width: childrenRect.width
                height: childrenRect.height
            }
            ShimmerText {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, 150)
                text: root.title
                active: root.loading
                color: root.quiet ? KodosiTheme.inkMuted : KodosiTheme.ink
                font.pixelSize: KodosiTheme.fontBody
                font.weight: Font.Medium
                elide: Text.ElideMiddle
            }
            Item {
                id: accessorySlot
                anchors.verticalCenter: parent.verticalCenter
                visible: children.length > 0 && children[0].visible
                width: childrenRect.width
                height: childrenRect.height
            }
            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 9
                height: 9
                name: "chevron-down"
                strokeWidth: 2.8
                color: root.open ? KodosiTheme.accentStrong : KodosiTheme.inkFaint
                rotation: root.open ? 180 : 0

                Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
            }
        }
    }

    KFocusIndicator {
        active: root.visualFocus
        color: KodosiTheme.accentStrong
    }
}
