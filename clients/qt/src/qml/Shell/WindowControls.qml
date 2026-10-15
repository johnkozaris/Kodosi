import Kodosi 1.0
import QtQuick
import QtQuick.Controls

Row {
    id: root

    readonly property var host: Window.window
    readonly property bool maximized: host !== null && host.visibility === Window.Maximized

    component Control: KIconButton {
        required property string label

        activeFocusOnTab: false
        focusPolicy: Qt.NoFocus
        Accessible.id: objectName
        Accessible.name: label
        ToolTip.visible: hovered
        ToolTip.text: label
        ToolTip.delay: 600
    }

    spacing: 4

    Control {
        glyph: "window-minimize"
        label: qsTr("Minimize")
        objectName: "window.minimize"
        onClicked: root.host.showMinimized()
    }
    Control {
        glyph: root.maximized ? "window-restore" : "window-maximize"
        label: root.maximized ? qsTr("Restore") : qsTr("Maximize")
        objectName: "window.maximize"
        onClicked: root.maximized ? root.host.showNormal() : root.host.showMaximized()
    }
    Control {
        glyph: "window-close"
        label: qsTr("Close")
        destructive: true
        objectName: "window.close"
        onClicked: root.host.close()
    }
}
