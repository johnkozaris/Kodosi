pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property bool loading: false
    property bool failed: false
    readonly property var sessions: Models.Sessions.sessions
    readonly property var places: {
        const groups = {}
        const order = []
        for (const session of sessions) {
            const local = session.kind === "local"
            const key = local ? "" : (session.ownerUserId || "") + ":" + session.hostLabel
            if (!groups[key]) {
                groups[key] = { key: key, label: local ? qsTr("This computer") : (session.isOwner || !session.ownerName ? session.hostLabel : session.ownerName + " · " + session.hostLabel), sessions: [] }
                order.push(key)
            }
            groups[key].sessions.push(session)
        }
        order.sort((left, right) => left === "" ? -1 : right === "" ? 1 : groups[left].label.localeCompare(groups[right].label))
        return order.map(key => groups[key])
    }

    signal newTerminalRequested

    objectName: "stage.overview"
    Accessible.id: objectName

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(420, parent.width - 48)
        visible: root.failed
        title: qsTr("The terminals did not load")
        message: Models.Sessions.authorityError
        art: Well {
            width: 52
            height: 52
            KIcon { anchors.centerIn: parent; width: 22; height: 22; name: "warning"; color: KodosiTheme.caution }
        }

        KButton {
            objectName: "stage.sessions.error.retry"
            Accessible.id: objectName
            variant: KButton.Primary
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: Models.SessionActions.refresh()
        }
    }
    ColumnLayout {
        anchors.centerIn: parent
        visible: root.loading && !root.failed
        spacing: 14

        CursorBlock { Layout.alignment: Qt.AlignHCenter; Layout.preferredWidth: 12; Layout.preferredHeight: 22; blinks: visible }
        ShimmerText {
            Layout.alignment: Qt.AlignHCenter
            objectName: "stage.sessions.loading"
            text: qsTr("Loading")
            active: visible
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
        }
    }
    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(460, parent.width - 48)
        visible: !root.loading && !root.failed && root.sessions.length === 0
        spacing: 26

        Item {
            Layout.alignment: Qt.AlignHCenter
            implicitWidth: 300
            implicitHeight: 150

            Raised {
                anchors.fill: parent
                radius: KodosiTheme.radiusTile
                fill: KodosiTheme.terminal
                rim: false
                elevation: 2
            }
            Rectangle {
                width: parent.width
                height: 30
                topLeftRadius: KodosiTheme.radiusTile
                topRightRadius: KodosiTheme.radiusTile
                color: KodosiTheme.terminalBand

                Row {
                    x: 10
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    AgentMark { size: 16 }
                    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 70; height: 6; radius: 3; color: KodosiTheme.alpha(KodosiTheme.terminalInk, 0.2) }
                }
            }
            Row {
                x: 16
                y: 48
                spacing: 8

                Text { text: "~ ❯"; color: KodosiTheme.terminalInkFaint; font.family: "monospace"; font.pixelSize: 13 }
                CursorBlock { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 15; blinks: true; color: KodosiTheme.terminalAccent }
            }
        }
        EmptyState {
            Layout.fillWidth: true
            title: qsTr("Start with a terminal")
            message: qsTr("Run a shell or an agent. Share it with a room when you want company.")

            KButton {
                objectName: "stage.empty.new"
                Accessible.id: objectName
                variant: KButton.Primary
                large: true
                iconName: "plus"
                text: qsTr("New terminal")
                onClicked: root.newTerminalRequested()
            }
        }
    }
    KScrollView {
        id: scroll
        anchors.fill: parent
        visible: !root.loading && !root.failed && root.sessions.length > 0
        contentWidth: availableWidth

        ColumnLayout {
            x: 30
            width: Math.min(1040, scroll.availableWidth - 60)
            spacing: 22

            ColumnLayout {
                Layout.topMargin: 44
                spacing: 4

                PlainLabel { text: qsTr("Your terminals"); font.pixelSize: KodosiTheme.fontLarge; font.weight: Font.DemiBold; font.letterSpacing: -0.6 }
                PlainLabel {
                    text: Identity.count(root.sessions.length, qsTr("1 terminal"), qsTr("%1 terminals")) + " · " + Identity.count(root.places.length, qsTr("1 computer"), qsTr("%1 computers"))
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
            }
            Repeater {
                model: root.places

                ColumnLayout {
                    id: place

                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 10

                    RowLayout {
                        spacing: 7
                        KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "laptop"; color: KodosiTheme.inkFaint }
                        PlainLabel { text: place.modelData.label; color: KodosiTheme.inkMuted; font.pixelSize: KodosiTheme.fontFootnote; font.weight: Font.Medium }
                    }
                    CardGrid {
                        id: grid
                        Layout.fillWidth: true

                        Repeater {
                            model: place.modelData.sessions

                            AbstractButton {
                                id: card

                                required property var modelData
                                readonly property bool needsYou: Models.Sessions.attention.indexOf(modelData.id) >= 0

                                width: grid.cardWidth
                                height: 62
                                hoverEnabled: true
                                activeFocusOnTab: true
                                objectName: "overview.session." + modelData.id
                                Accessible.id: objectName
                                Accessible.name: modelData.name
                                scale: pressed ? 0.98 : 1

                                Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover } }

                                background: Raised {
                                    radius: KodosiTheme.radiusLg
                                    fill: card.hovered ? KodosiTheme.lifted : KodosiTheme.raised
                                    elevation: card.hovered ? 2 : 1
                                }
                                contentItem: RowLayout {
                                    spacing: 12

                                    AgentMark { Layout.leftMargin: 12; program: card.modelData.program || ""; size: 32; session: card.modelData }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 2
                                        PlainLabel { Layout.fillWidth: true; text: card.modelData.name; font.weight: Font.DemiBold; elide: Text.ElideRight }
                                        ActivityLine {
                                            Layout.fillWidth: true
                                            session: card.modelData
                                            words: card.modelData.activity || card.modelData.folderName || Identity.kindLabel(Identity.kind(card.modelData.program || ""))
                                        }
                                    }
                                    StatusSign { Layout.rightMargin: 12; session: card.modelData; unseen: card.needsYou; size: 16 }
                                }
                                onClicked: {
                                    Models.Sessions.clearAttention(modelData.id)
                                    Models.SessionActions.activate(modelData.id)
                                }
                            }
                        }
                        AbstractButton {
                            id: slot
                            visible: place.modelData.key === ""
                            width: grid.cardWidth
                            height: 62
                            hoverEnabled: true
                            activeFocusOnTab: true
                            objectName: "overview.new"
                            Accessible.id: objectName
                            Accessible.name: qsTr("New terminal")
                            background: Rectangle {
                                radius: KodosiTheme.radiusLg
                                color: KodosiTheme.alpha(KodosiTheme.ink, slot.hovered ? 0.05 : 0)
                                border.width: 1
                                border.color: KodosiTheme.alpha(KodosiTheme.inkFaint, slot.hovered ? 0.8 : 0.45)
                            }
                            contentItem: RowLayout {
                                spacing: 8
                                Item { Layout.fillWidth: true }
                                KIcon { Layout.preferredWidth: 12; Layout.preferredHeight: 12; name: "prompt"; strokeWidth: 2.6; color: KodosiTheme.inkFaint }
                                CursorBlock { Layout.preferredWidth: 7; Layout.preferredHeight: 14; blinks: slot.hovered }
                                PlainLabel { text: qsTr("New terminal"); color: KodosiTheme.inkMuted; font.weight: Font.DemiBold }
                                Item { Layout.fillWidth: true }
                            }
                            onClicked: root.newTerminalRequested()
                        }
                    }
                }
            }
            Item { Layout.preferredHeight: 30 }
        }
    }
}
