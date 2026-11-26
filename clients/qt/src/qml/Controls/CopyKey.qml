import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

Raised {
    id: root

    property string message: ""
    property bool shown: false
    property bool copied: false
    readonly property bool showing: enabled && (shown || key.hovered || key.activeFocus)

    implicitWidth: key.width + 4
    implicitHeight: key.height + 4
    radius: height / 2
    fill: look.lifted
    elevation: 2
    opacity: showing ? 1 : 0
    scale: showing ? 1 : 0.94
    transformOrigin: Item.Right
    onOpacityChanged: {
        if (opacity === 0)
            copied = false
    }

    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }
    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

    KIconButton {
        id: key

        x: 2
        y: 2
        glyph: root.copied ? "check" : "copy"
        size: 24
        active: root.copied
        terminalLook: root.look
        // A click leaves the focus where it was, such as in the composer; Tab still reaches the key.
        focusPolicy: Qt.TabFocus
        // While hidden, the key lets clicks and touches through to the message under it.
        containmentMask: root.showing ? null : nowhere
        Accessible.id: root.objectName
        Accessible.name: qsTr("Copy")
        Accessible.description: root.copied ? qsTr("Copied") : ""
        ToolTip.text: root.copied ? qsTr("Copied") : qsTr("Copy")
        onClicked: {
            Models.DesktopFiles.copyText(root.message)
            root.copied = true
        }
    }
    QtObject {
        id: nowhere

        function contains(spot: point): bool {
            return false
        }
    }
}
