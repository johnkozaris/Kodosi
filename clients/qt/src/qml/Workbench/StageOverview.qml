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
    property var shownPlaces: ({})
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

    signal resumeRequested

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
    // The launcher stands a little above the middle, and a list that it opens grows down from there.
    AgentLauncher {
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(40, parent.height / 2 - 150)
        width: Math.min(560, parent.width - 48)
        visible: !root.loading && !root.failed && root.sessions.length === 0
        welcome: true
        onResumeRequested: root.resumeRequested()
    }
    KScrollView {
        id: scroll
        anchors.fill: parent
        visible: !root.loading && !root.failed && root.sessions.length > 0
        contentWidth: availableWidth

        ColumnLayout {
            x: (scroll.availableWidth - Math.min(1040, scroll.availableWidth)) / 2 + 36
            width: Math.min(1040, scroll.availableWidth) - 72
            spacing: 22

            AgentLauncher {
                Layout.topMargin: 44
                Layout.fillWidth: true
                onResumeRequested: root.resumeRequested()
            }
            ColumnLayout {
                Layout.topMargin: 8
                spacing: 4

                PlainLabel { text: qsTr("Your terminals"); font.pixelSize: KodosiTheme.fontLarge; font.weight: Font.DemiBold; font.letterSpacing: -0.6 }
                PlainLabel {
                    text: Identity.count(root.sessions.length, qsTr("1 terminal"), qsTr("%1 terminals")) + " · " + Identity.count(root.places.length, qsTr("1 computer"), qsTr("%1 computers"))
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
            }
            Repeater {
                model: root.places.length

                Arrival {
                    id: place

                    required property int index
                    readonly property var group: root.places[index] || ({ key: "", label: "", sessions: [] })

                    Layout.fillWidth: true
                    seen: root.shownPlaces
                    key: "place:" + group.key
                    delay: 40 * (index + 1)
                    spacing: 12

                    RowLayout {
                        spacing: 7
                        KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "laptop"; color: KodosiTheme.inkMuted }
                        PlainLabel { text: place.group.label; color: KodosiTheme.inkMuted; font.weight: Font.DemiBold; font.letterSpacing: -0.08 }
                    }
                    CardGrid {
                        id: grid
                        Layout.fillWidth: true
                        minimum: 230
                        maximum: 320

                        Repeater {
                            model: place.group.sessions.length

                            AbstractButton {
                                id: card

                                required property int index
                                readonly property var session: place.group.sessions[index] || ({})
                                readonly property bool needsYou: Models.Sessions.attention.indexOf(session.id) >= 0

                                width: grid.cardWidth
                                height: 68
                                hoverEnabled: true
                                activeFocusOnTab: true
                                objectName: "overview.session." + (session.id || "")
                                Accessible.id: objectName
                                Accessible.name: session.name || ""
                                scale: pressed ? 0.98 : 1
                                transform: Translate { y: card.hovered ? -1 : 0; Behavior on y { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } } }

                                Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                                background: Raised {
                                    radius: KodosiTheme.radiusLg
                                    fill: card.hovered ? KodosiTheme.lifted : KodosiTheme.raised
                                    elevation: card.hovered ? 2 : 1
                                }
                                contentItem: RowLayout {
                                    spacing: 12

                                    AgentMark { Layout.leftMargin: 14; program: card.session.program || ""; size: 34; session: card.session }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 3
                                        PlainLabel { Layout.fillWidth: true; text: card.session.name || ""; font.weight: Font.DemiBold; font.letterSpacing: -0.08; elide: Text.ElideRight }
                                        ActivityLine {
                                            Layout.fillWidth: true
                                            session: card.session
                                            words: card.session.activity || card.session.folderName || Identity.kindLabel(Identity.kind(card.session.program || ""))
                                        }
                                    }
                                    StatusSign { Layout.rightMargin: 14; session: card.session; unseen: card.needsYou; size: 13 }
                                }
                                onClicked: {
                                    Models.Sessions.clearAttention(session.id)
                                    Models.SessionActions.activate(session.id)
                                }
                            }
                        }
                    }
                }
            }
            Item { Layout.preferredHeight: 36 }
        }
    }
}
