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
    readonly property bool lit: hovered || menu.visible || selected

    signal aboutRequested
    signal resumeRequested(string provider)
    signal startRequested

    function start(agent, account) {
        menu.dismiss()
        Models.DesktopSettings.lastComputer = ""
        Models.Launcher.chooseAgent(agent)
        Models.Launcher.choose("account", agent, account)
        root.startRequested()
    }

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
            menu.open()
    }

    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    background: Bezel {
        radius: KodosiTheme.radiusMd
        quiet: true
        pressed: root.pressed
        fill: KodosiTheme.buttonFace(root.lit)
    }
    contentItem: Item {
        KIcon {
            anchors.centerIn: parent
            width: 16
            height: 16
            name: "sliders"
            strokeWidth: 2
            color: root.lit ? KodosiTheme.ink : KodosiTheme.inkMuted
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

        x: root.opensRight ? 0 : root.width - width
        y: -height - 6
        width: 224
        cascade: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        transformOrigin: root.opensRight ? Popup.BottomLeft : Popup.BottomRight
        onAboutToShow: {
            Models.Launcher.refresh(false)
            Models.Launcher.refreshUsage()
        }

        KMenuItem {
            text: qsTr("Settings")
            iconName: "gear"
            hint: Models.Shortcuts.hints.settings
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
        UsageMenu {
            onStartRequested: (agent, account) => root.start(agent, account)
        }
        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
        PlainLabel {
            leftPadding: 10
            topPadding: 4
            bottomPadding: 2
            text: qsTr("Resume")
            color: KodosiTheme.inkFaint
            font.pixelSize: KodosiTheme.fontCaption
            font.weight: Font.Medium
        }
        KMenuItem {
            text: Models.DesktopSettings.agentLabel("claude")
            agent: "claude"
            objectName: "sidebar.resume.claude"
            onTriggered: root.resumeRequested("claude")
        }
        KMenuItem {
            text: Models.DesktopSettings.agentLabel("copilot")
            agent: "copilot"
            objectName: "sidebar.resume.copilot"
            onTriggered: root.resumeRequested("copilot")
        }
        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
        KMenuItem {
            text: qsTr("About Kodosi")
            iconName: "info"
            objectName: "sidebar.about"
            onTriggered: root.aboutRequested()
        }
        MenuSeparator {
            visible: Models.Account.signedIn
            height: visible ? implicitHeight : 0
            contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
        }
        KMenuItem {
            visible: Models.Account.signedIn
            height: visible ? implicitHeight : 0
            text: qsTr("Sign out…")
            iconName: "leave"
            objectName: "sidebar.signOut"
            onTriggered: signOut.open()
        }
    }
    SignOutDialog {
        id: signOut
    }
}
