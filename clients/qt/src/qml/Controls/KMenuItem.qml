import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

MenuItem {
    id: root

    property string iconName: ""
    property bool destructive: false
    readonly property color tone: !enabled ? KodosiTheme.inkFaint : destructive ? KodosiTheme.danger : KodosiTheme.ink

    implicitHeight: 32
    leftPadding: 10
    rightPadding: 12

    indicator: Item {}
    contentItem: RowLayout {
        spacing: 9

        KIcon {
            visible: root.iconName.length > 0
            Layout.preferredWidth: 14
            Layout.preferredHeight: 14
            name: root.iconName
            color: root.destructive ? KodosiTheme.danger : root.highlighted ? KodosiTheme.ink : KodosiTheme.inkMuted
        }
        PlainLabel {
            Layout.fillWidth: true
            text: root.text
            color: root.tone
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        KIcon {
            visible: root.checkable && root.checked
            Layout.preferredWidth: 12
            Layout.preferredHeight: 12
            name: "check"
            strokeWidth: 2.4
            color: KodosiTheme.accentStrong
        }
    }
    background: Rectangle {
        radius: KodosiTheme.radiusSm
        color: root.highlighted && root.enabled ? (root.destructive ? KodosiTheme.dangerSoft : KodosiTheme.accentSoft) : "transparent"
    }
}
