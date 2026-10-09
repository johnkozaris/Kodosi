pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property bool changed: family.text !== Models.DesktopSettings.fontFamily || size.value !== Models.DesktopSettings.fontSize
        || cursorStyle !== Models.DesktopSettings.cursorStyle || scrollback.value !== Models.DesktopSettings.scrollbackLines
        || blink.checked !== Models.DesktopSettings.cursorBlink || lineHeight.value !== storedLineHeight
    readonly property int storedLineHeight: Math.round(Models.DesktopSettings.lineHeight * 100)
    property int cursorStyle: Models.DesktopSettings.cursorStyle

    function revert() {
        family.text = Models.DesktopSettings.fontFamily;
        size.value = Models.DesktopSettings.fontSize;
        lineHeight.value = storedLineHeight;
        cursorStyle = Models.DesktopSettings.cursorStyle;
        scrollback.value = Models.DesktopSettings.scrollbackLines;
        blink.checked = Models.DesktopSettings.cursorBlink;
    }

    spacing: 18

    Connections {
        function onSettingsChanged() {
            root.revert();
        }

        target: Models.DesktopSettings
    }
    Item {
        Layout.fillWidth: true
        implicitHeight: 104

        Raised { anchors.fill: parent; radius: KodosiTheme.radiusTile; fill: KodosiTheme.terminal; rim: false; elevation: 2 }
        Column {
            x: 16
            y: 14
            spacing: 2 + Math.max(0, Math.round((lineHeight.value / 100 - 1) * size.value))

            Row {
                spacing: size.value * 0.6
                Text { text: "~/kodosi ❯"; color: KodosiTheme.terminalInkFaint; font.family: family.text; font.pixelSize: size.value }
                Text { text: "claude"; color: KodosiTheme.terminalInk; font.family: family.text; font.pixelSize: size.value }
            }
            Text { text: "✻ Welcome back. What are we building?"; color: KodosiTheme.terminalAccent; font.family: family.text; font.pixelSize: size.value }
            Row {
                spacing: size.value * 0.6
                Text { text: "~/kodosi ❯"; color: KodosiTheme.terminalInkFaint; font.family: family.text; font.pixelSize: size.value }
                CursorBlock {
                    anchors.bottom: parent.bottom
                    width: root.cursorStyle === 1 ? 2 : size.value * 0.6
                    height: root.cursorStyle === 2 ? 2 : size.value * 1.15
                    blinks: blink.checked
                    color: KodosiTheme.terminalAccent
                }
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true

        ListRow {
            iconName: "sun"
            tint: "#54433a"
            title: qsTr("Appearance")

            SegmentedPill {
                compact: true
                identifier: "settings.appearance"
                options: [
                    { value: Models.Appearance.System, label: qsTr("Auto") },
                    { value: Models.Appearance.Light, label: qsTr("Light"), icon: "sun" },
                    { value: Models.Appearance.Dark, label: qsTr("Dark"), icon: "moon" }
                ]
                currentValue: Models.Appearance.preference
                onActivated: value => Models.Appearance.setPreference(value)
            }
        }
        ListRow {
            iconName: "document"
            tint: "#607fcc"
            title: qsTr("Font")

            KTextField {
                id: family

                Accessible.id: objectName
                Accessible.name: qsTr("Terminal font")
                Layout.preferredWidth: 220
                Layout.preferredHeight: 30
                objectName: "panel.settingsView.family"
                text: Models.DesktopSettings.fontFamily
            }
        }
        ListRow {
            iconName: "focus"
            tint: "#4f9bb0"
            title: qsTr("Size")

            KSpinBox {
                id: size

                Accessible.id: objectName
                Accessible.name: qsTr("Terminal font size")
                from: 8
                objectName: "panel.settingsView.size"
                to: 32
                value: Models.DesktopSettings.fontSize
            }
        }
        ListRow {
            iconName: "line-height"
            tint: "#8a6fc2"
            title: qsTr("Line height")

            KSpinBox {
                id: lineHeight

                Accessible.id: objectName
                Accessible.name: qsTr("Terminal line height")
                editable: false
                from: 80
                objectName: "panel.settingsView.lineHeight"
                stepSize: 5
                to: 200
                value: root.storedLineHeight
                textFromValue: value => (value / 100).toFixed(2)
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true

        ListRow {
            iconName: "prompt"
            tint: "#b8743c"
            title: qsTr("Cursor")

            SegmentedPill {
                compact: true
                identifier: "panel.settingsView.cursor"
                options: [
                    { value: 0, label: qsTr("Block") },
                    { value: 1, label: qsTr("Bar") },
                    { value: 2, label: qsTr("Underline") }
                ]
                currentValue: root.cursorStyle
                onActivated: value => root.cursorStyle = value
            }
        }
        ListRow {
            iconName: "agent"
            tint: "#d9a441"
            title: qsTr("Blink the cursor")

            KSwitch {
                id: blink

                Accessible.id: objectName
                Accessible.name: qsTr("Blink the cursor")
                checked: Models.DesktopSettings.cursorBlink
                objectName: "panel.settingsView.blink"
            }
        }
        ListRow {
            iconName: "chevron-up"
            tint: "#7dbb99"
            title: qsTr("Scrollback")
            subtitle: qsTr("Lines kept for each terminal")

            KSpinBox {
                id: scrollback

                Accessible.id: objectName
                Accessible.name: qsTr("Scrollback lines")
                implicitWidth: 150
                from: 100
                objectName: "panel.settingsView.scrollback"
                stepSize: 1000
                to: 100000
                value: Models.DesktopSettings.scrollbackLines
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        PlainLabel {
            Layout.fillWidth: true
            color: KodosiTheme.danger
            font.pixelSize: KodosiTheme.fontFootnote
            text: Models.DesktopSettings.settingsError
            wrapMode: Text.WordWrap
        }
        KButton {
            Accessible.id: objectName
            objectName: "panel.settingsView.reset"
            text: qsTr("Defaults")
            variant: KButton.Ghost

            onClicked: Models.DesktopSettings.resetTerminal()
        }
        KButton {
            variant: KButton.Ghost
            text: qsTr("Revert")
            visible: root.changed
            onClicked: root.revert()
        }
        KButton {
            Accessible.id: objectName
            objectName: "panel.settingsView.apply-terminal-settings"
            variant: KButton.Primary
            enabled: root.changed
            text: root.changed ? qsTr("Apply") : qsTr("Applied")

            onClicked: Models.DesktopSettings.apply(family.text, size.value, root.cursorStyle, lineHeight.value / 100, scrollback.value, blink.checked)
        }
    }
}
