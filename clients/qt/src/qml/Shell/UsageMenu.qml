pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

// How much of each signed-in account's plan limits is used, one dial for each limit, as a submenu of
// the sidebar menu. A click on an account opens the launcher with it.
KMenu {
    id: root

    readonly property var sections: Models.Launcher.usageSections
    readonly property real dial: 26

    signal startRequested(string agent, string account)

    title: qsTr("Provider Usage")
    width: 284
    available: Models.Launcher.usageAvailable
    mark: Component {
        UsageRing {
            width: 14
            height: 14
            limit: Models.Launcher.usageNearest ?? null
            lineWidth: 1.75
        }
    }

    Column {
        width: root.availableWidth
        objectName: "sidebar.usage.panel"
        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Provider Usage")

        PlainLabel {
            visible: root.sections.length === 0
            leftPadding: 8
            height: 30
            verticalAlignment: Text.AlignVCenter
            text: qsTr("No plan limits to show")
            color: KodosiTheme.inkFaint
            font.pixelSize: KodosiTheme.fontFootnote
        }
        Repeater {
            model: root.sections

            delegate: Column {
                id: section

                required property var modelData
                required property int index

                width: parent.width

                Rectangle {
                    visible: section.index > 0
                    x: 8
                    width: parent.width - 16
                    height: 9
                    color: "transparent"

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width
                        height: 1
                        color: KodosiTheme.hairline
                    }
                }
                Item {
                    width: parent.width
                    height: 24

                    Row {
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        AgentMark {
                            anchors.verticalCenter: parent.verticalCenter
                            program: section.modelData.agent
                            size: 15
                        }
                        PlainLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            text: Models.DesktopSettings.agentLabel(section.modelData.agent)
                            color: KodosiTheme.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                            font.weight: Font.Medium
                        }
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter

                        Repeater {
                            model: section.modelData.windows

                            delegate: PlainLabel {
                                required property var modelData

                                width: root.dial + 6
                                horizontalAlignment: Text.AlignHCenter
                                text: modelData.window
                                color: KodosiTheme.inkFaint
                                font.pixelSize: KodosiTheme.fontCaption
                                fontSizeMode: Text.HorizontalFit
                                minimumPixelSize: Math.round(KodosiTheme.fontCaption * 0.8)
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
                Repeater {
                    model: section.modelData.rows

                    delegate: AbstractButton {
                        id: account

                        required property var modelData
                        readonly property var usage: modelData.usage ?? null
                        readonly property bool stale: !!usage && Date.now() - usage.readAtMs > 600000

                        width: section.width
                        height: 38
                        hoverEnabled: true
                        activeFocusOnTab: true
                        objectName: "sidebar.usage." + section.modelData.agent + "." + modelData.name
                        Accessible.role: Accessible.Button
                        Accessible.name: [modelData.title, Models.DesktopSettings.agentLabel(section.modelData.agent)]
                            .concat(usage ? usage.limits.map(limit => limit.spoken) : []).join(", ")
                        Accessible.description: qsTr("Opens the launcher with this account")
                        ToolTip.visible: hovered
                        ToolTip.text: modelData.identity
                        ToolTip.delay: Tip.delay
                        onClicked: root.startRequested(section.modelData.agent, modelData.name)
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()

                        background: Rectangle {
                            radius: KodosiTheme.radiusSm
                            color: account.hovered || account.visualFocus ? KodosiTheme.mix(KodosiTheme.lifted, KodosiTheme.ink, 0.06) : "transparent"

                            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                        }
                        contentItem: Item {
                            opacity: account.stale ? 0.55 : 1

                            Row {
                                x: 8
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 9

                                AccountMark {
                                    anchors.verticalCenter: parent.verticalCenter
                                    initial: account.modelData.title.charAt(0)
                                    key: account.modelData.key
                                    signedIn: true
                                    size: 20
                                }
                                PlainLabel {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: account.width - 16 - 29 - dials.width - 10
                                    text: account.modelData.title
                                    color: KodosiTheme.ink
                                    font.weight: Font.Medium
                                    elide: Text.ElideMiddle
                                }
                            }
                            Row {
                                id: dials

                                anchors.right: parent.right
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter

                                Repeater {
                                    model: section.modelData.windows.length > 0 ? section.modelData.windows : [null]

                                    delegate: Item {
                                        id: slot

                                        required property var modelData
                                        readonly property var limit: account.usage && modelData
                                            ? account.usage.limits.find(entry => entry.id === modelData.id) ?? null : null

                                        width: root.dial + 6
                                        height: root.dial

                                        UsageDial {
                                            anchors.centerIn: parent
                                            visible: slot.limit !== null || !account.usage
                                            limit: slot.limit
                                            size: root.dial
                                        }
                                    }
                                }
                            }
                        }
                        KFocusIndicator { active: account.visualFocus }
                    }
                }
            }
        }
    }
}
