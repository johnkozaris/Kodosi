pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls

ComboBox {
    id: root

    implicitHeight: KodosiTheme.controlHeight
    leftPadding: 11
    rightPadding: 34
    font.pixelSize: 12

    contentItem: PlainLabel {
        leftPadding: 0
        rightPadding: 0
        text: root.displayText
        color: root.enabled
            ? KodosiTheme.textPrimary
            : KodosiTheme.disabled
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    indicator: KIcon {
        x: root.width - width - 10
        y: Math.round((root.height - height) / 2)
        width: 14
        height: 14
        name: root.popup.visible ? "chevron-up" : "chevron-down"
        color: root.activeFocus
            ? KodosiTheme.accent
            : KodosiTheme.textSecondary
    }

    KFocusIndicator { active: root.activeFocus && root.enabled }

    background: Rectangle {
        color: root.pressed
            ? KodosiTheme.surfaceSelected
            : KodosiTheme.input
        radius: KodosiTheme.radiusSmall
    }

    delegate: KMenuItem {
        required property int index
        width: ListView.view.width
        text: root.textAt(index)
        highlighted: root.highlightedIndex === index
    }

    popup: Popup {
        y: root.height + 4
        width: root.width
        implicitHeight: Math.min(contentItem.implicitHeight + 12, 260)
        padding: 6

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: root.popup.visible ? root.delegateModel : null
            currentIndex: root.highlightedIndex
            spacing: 2
            KScrollBar.vertical: KScrollBar {}
        }

        background: Rectangle {
            color: KodosiTheme.surfaceRaised
            radius: KodosiTheme.radiusLarge
        }
    }
}
