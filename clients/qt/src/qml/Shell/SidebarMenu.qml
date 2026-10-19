pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

AbstractButton {
    id: root

    property bool selected: false
    property int size: 34
    property bool opensRight: false

    signal aboutRequested

    implicitWidth: size
    implicitHeight: size
    hoverEnabled: true
    activeFocusOnTab: true
    scale: pressed ? 0.95 : 1
    objectName: "sidebar.menu"
    Accessible.id: objectName
    Accessible.name: qsTr("Settings and more")
    ToolTip.visible: hovered && !menu.visible
    ToolTip.text: Accessible.name
    ToolTip.delay: Tip.delay
    onClicked: {
        if (menu.visible)
            menu.close()
        else
            menu.popup(root, root.opensRight ? 0 : root.width - menu.width, -menu.implicitHeight - 6)
    }

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Bezel {
        radius: KodosiTheme.radiusMd
        quiet: true
        pressed: root.pressed
        fill: root.selected || menu.visible ? KodosiTheme.accentSoft : KodosiTheme.buttonFace(root.hovered)
    }
    contentItem: Item {
        KIcon {
            anchors.centerIn: parent
            width: 16
            height: 16
            name: "more"
            strokeWidth: 2.2
            color: root.selected || menu.visible ? KodosiTheme.accentStrong : root.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
        }
    }

    CountBadge {
        x: parent.width - width + 5
        y: -5
        count: Models.People.incoming.length
    }
    KFocusIndicator { active: root.visualFocus }

    KMenu {
        id: menu

        width: 224
        transformOrigin: root.opensRight ? Popup.BottomLeft : Popup.BottomRight

        KMenuItem {
            text: qsTr("Settings")
            iconName: "settings"
            hint: qsTr("Ctrl+,")
            objectName: "sidebar.settings"
            onTriggered: Models.DesktopState.activeView = Models.DesktopState.Settings
        }
        KMenuItem {
            text: qsTr("Friends")
            iconName: "people"
            badge: Models.People.incoming.length
            objectName: "sidebar.people"
            onTriggered: Models.DesktopState.activeView = Models.DesktopState.People
        }
        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
        KMenuItem {
            text: qsTr("About Kodosi")
            iconName: "info"
            objectName: "sidebar.about"
            onTriggered: root.aboutRequested()
        }
    }
}
