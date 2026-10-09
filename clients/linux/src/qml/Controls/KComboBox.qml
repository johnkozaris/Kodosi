pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls

ComboBox {
    id: root

    implicitHeight: 30
    implicitWidth: Math.max(120, label.implicitWidth + 46)
    leftPadding: 14
    rightPadding: 30
    hoverEnabled: true
    font.pixelSize: KodosiTheme.fontBody

    contentItem: PlainLabel {
        id: label
        text: root.displayText
        color: root.enabled ? KodosiTheme.ink : KodosiTheme.inkFaint
        font.weight: Font.Medium
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    indicator: KIcon {
        x: root.width - width - 11
        y: Math.round((root.height - height) / 2)
        width: 11
        height: 11
        name: "chevron-down"
        strokeWidth: 2.2
        color: KodosiTheme.inkMuted
        rotation: root.popup.visible ? 180 : 0

        Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
    }
    background: Item {
        Raised {
            anchors.fill: parent
            radius: height / 2
            fill: root.hovered || root.popup.visible ? KodosiTheme.lifted : KodosiTheme.raised
        }
        KFocusIndicator { active: root.visualFocus && root.enabled }
    }
    delegate: KMenuItem {
        required property int index

        width: ListView.view.width
        text: root.textAt(index)
        highlighted: root.highlightedIndex === index
        checkable: true
        checked: root.currentIndex === index
    }
    popup: Popup {
        y: root.height + 6
        width: Math.max(root.width, 180)
        implicitHeight: Math.min(contentItem.implicitHeight + 12, 300)
        padding: 6

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: root.popup.visible ? root.delegateModel : null
            currentIndex: root.highlightedIndex
            spacing: 1
            ScrollBar.vertical: KScrollBar {}
        }
        background: Raised {
            radius: KodosiTheme.radiusLg
            fill: KodosiTheme.lifted
            elevation: 2
        }
        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionHover }
            NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic }
        }
    }
}
