import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    spacing: 12

    GridLayout {
        Layout.fillWidth: true
        columnSpacing: 18
        columns: 2
        rowSpacing: 10

        PlainLabel { color: KodosiTheme.textSecondary; text: qsTr("Font") }
        KTextField {
            id: family

            Accessible.id: objectName
            Accessible.name: qsTr("Terminal font")
            Layout.fillWidth: true
            objectName: "panel.settingsView.family"
            text: Models.DesktopSettings.fontFamily
        }
        PlainLabel { color: KodosiTheme.textSecondary; text: qsTr("Size") }
        KSpinBox {
            id: size

            Accessible.id: objectName
            Accessible.name: qsTr("Terminal font size")
            from: 8
            objectName: "panel.settingsView.size"
            to: 32
            value: Models.DesktopSettings.fontSize
        }
        PlainLabel { color: KodosiTheme.textSecondary; text: qsTr("Cursor") }
        KComboBox {
            id: cursor

            Accessible.id: objectName
            Accessible.name: qsTr("Cursor style")
            currentIndex: Models.DesktopSettings.cursorStyle
            model: [qsTr("Block"), qsTr("Bar"), qsTr("Underline")]
            objectName: "panel.settingsView.cursor"
        }
        PlainLabel { color: KodosiTheme.textSecondary; text: qsTr("Scrollback") }
        KSpinBox {
            id: scrollback

            Accessible.id: objectName
            Accessible.name: qsTr("Scrollback lines")
            editable: true
            from: 100
            objectName: "panel.settingsView.scrollback"
            stepSize: 100
            to: 100000
            value: Models.DesktopSettings.scrollbackLines
        }
        PlainLabel { color: KodosiTheme.textSecondary; text: qsTr("Blink cursor") }
        KCheckBox {
            id: blink

            Accessible.id: objectName
            Accessible.name: qsTr("Blink cursor")
            checked: Models.DesktopSettings.cursorBlink
            objectName: "panel.settingsView.blink"
        }
    }
    RowLayout {
        KButton {
            Accessible.id: objectName
            objectName: "panel.settingsView.apply-terminal-settings"
            text: qsTr("Apply")

            onClicked: Models.DesktopSettings.apply(family.text, size.value, cursor.currentIndex, Models.DesktopSettings.lineHeight, scrollback.value, blink.checked)
        }
        KButton {
            Accessible.id: objectName
            objectName: "panel.settingsView.reset"
            text: qsTr("Reset")
            variant: KButton.Quiet

            onClicked: Models.DesktopSettings.resetTerminal()
        }
    }
    PlainLabel {
        color: KodosiTheme.danger
        text: Models.DesktopSettings.settingsError
        visible: Models.DesktopSettings.settingsError.length > 0
    }
}
