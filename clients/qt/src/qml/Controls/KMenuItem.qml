import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

MenuItem {
    id: root

    property string iconName: ""
    property string detail: ""
    property bool destructive: false
    readonly property color tone: !enabled ? KodosiTheme.inkFaint : destructive ? KodosiTheme.danger : KodosiTheme.ink

    implicitHeight: detail.length > 0 ? 44 : 32
    leftPadding: 10
    rightPadding: 12

    Accessible.checkable: root.checkable || root.checked
    Accessible.checked: root.checked
    indicator: Item {}
    arrow: Item {}
    contentItem: RowLayout {
        spacing: 9

        KIcon {
            visible: root.iconName.length > 0
            Layout.preferredWidth: 14
            Layout.preferredHeight: 14
            name: root.iconName
            color: root.destructive ? KodosiTheme.danger : root.highlighted ? KodosiTheme.ink : KodosiTheme.inkMuted
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1

            PlainLabel {
                Layout.fillWidth: true
                text: root.text
                color: root.tone
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            PlainLabel {
                Layout.fillWidth: true
                visible: root.detail.length > 0
                text: root.detail
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideMiddle
            }
        }
        KIcon {
            visible: root.checked
            Layout.preferredWidth: 12
            Layout.preferredHeight: 12
            name: "check"
            strokeWidth: 2.4
            color: KodosiTheme.accentStrong
        }
        KIcon {
            visible: root.subMenu !== null
            Layout.preferredWidth: 10
            Layout.preferredHeight: 10
            name: "prompt"
            strokeWidth: 2.4
            color: KodosiTheme.inkMuted
        }
    }
    background: Rectangle {
        radius: KodosiTheme.radiusSm
        color: root.highlighted && root.enabled ? (root.destructive ? KodosiTheme.dangerSoft : KodosiTheme.accentSoft) : "transparent"
    }
}
