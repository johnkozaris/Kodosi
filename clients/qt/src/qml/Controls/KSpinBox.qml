import Kodosi 1.0
import QtQuick
import QtQuick.Controls

SpinBox {
    id: root

    implicitHeight: 30
    implicitWidth: 124
    editable: true
    hoverEnabled: true

    contentItem: TextInput {
        z: 2
        text: root.displayText
        color: root.enabled ? KodosiTheme.ink : KodosiTheme.inkFaint
        selectionColor: KodosiTheme.accent
        selectedTextColor: KodosiTheme.accentInk
        font.pixelSize: KodosiTheme.fontBody
        font.weight: Font.DemiBold
        horizontalAlignment: Qt.AlignHCenter
        verticalAlignment: Qt.AlignVCenter
        readOnly: !root.editable
        validator: root.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
    }
    up.indicator: Item {
        x: root.width - width
        width: 34
        height: root.height

        KIcon {
            anchors.centerIn: parent
            width: 11
            height: 11
            name: "plus"
            strokeWidth: 2.2
            color: root.up.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
        }
    }
    down.indicator: Item {
        width: 34
        height: root.height

        KIcon {
            anchors.centerIn: parent
            width: 11
            height: 11
            name: "minus"
            strokeWidth: 2.2
            color: root.down.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
        }
    }
    background: Item {
        Raised {
            anchors.fill: parent
            radius: height / 2
        }
        KFocusIndicator { active: root.activeFocus && root.enabled }
    }
}
