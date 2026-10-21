pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

MenuItem {
    id: root

    property string iconName: ""
    property string agent: ""
    property string hint: ""
    property int badge: 0
    property bool destructive: false
    readonly property color tone: !enabled ? KodosiTheme.inkFaint : destructive ? KodosiTheme.danger : KodosiTheme.ink

    implicitHeight: 32
    leftPadding: 8
    rightPadding: 12

    indicator: Item {}
    contentItem: RowLayout {
        spacing: 9

        Item {
            visible: root.iconName.length > 0 || root.agent.length > 0
            Layout.preferredWidth: 18
            Layout.preferredHeight: 18

            KIcon {
                anchors.centerIn: parent
                width: 14
                height: 14
                visible: root.agent.length === 0
                name: root.iconName
                color: !root.enabled ? KodosiTheme.inkFaint : root.destructive ? KodosiTheme.danger : root.highlighted ? KodosiTheme.ink : KodosiTheme.inkMuted
            }
            Loader {
                anchors.fill: parent
                active: root.agent.length > 0
                sourceComponent: AgentMark {
                    program: root.agent
                    size: 18
                }
            }
        }
        PlainLabel {
            Layout.fillWidth: true
            text: root.text
            color: root.tone
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        PlainLabel {
            visible: root.hint.length > 0
            text: root.hint
            color: KodosiTheme.inkFaint
            font.pixelSize: KodosiTheme.fontCaption
        }
        CountBadge {
            count: root.badge
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
        color: root.highlighted && root.enabled ? KodosiTheme.mix(KodosiTheme.lifted, KodosiTheme.ink, 0.06) : "transparent"
    }
}
