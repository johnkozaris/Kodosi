pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
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
    signal resumeRequested

    component Prompt: Row {
        id: prompt

        property real size: 15

        spacing: size * 0.4

        Shape {
            id: chevron
            anchors.verticalCenter: parent.verticalCenter
            width: prompt.size * 0.42
            height: prompt.size * 0.72
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                strokeColor: KodosiTheme.inkFaint
                strokeWidth: Math.max(1.8, prompt.size * 0.13)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                startX: chevron.width * 0.2
                startY: chevron.height * 0.12
                PathLine { x: chevron.width * 0.82; y: chevron.height / 2 }
                PathLine { x: chevron.width * 0.2; y: chevron.height * 0.88 }
            }
        }
        CursorBlock {
            anchors.verticalCenter: parent.verticalCenter
            width: prompt.size * 0.55
            height: prompt.size
            blinks: true
        }
    }

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

        Arrival {
            Layout.alignment: Qt.AlignHCenter

            AbstractButton {
                id: firstSlot
                implicitWidth: 400
                implicitHeight: 220
                hoverEnabled: true
                activeFocusOnTab: true
                objectName: "stage.empty.new"
                Accessible.id: objectName
                Accessible.name: qsTr("New terminal")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("New terminal (Ctrl+Shift+N)")
                ToolTip.delay: 600
                scale: pressed ? 0.98 : 1

                Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                background: DashedSlot {
                    radius: KodosiTheme.radiusXl
                    hovered: firstSlot.hovered
                }
                contentItem: Item {
                    Row {
                        x: 18
                        y: 18
                        spacing: 6

                        Repeater {
                            model: 3

                            Rectangle { width: 7; height: 7; radius: 3.5; color: KodosiTheme.alpha(KodosiTheme.inkFaint, 0.35) }
                        }
                    }
                    Prompt {
                        x: 18
                        anchors.verticalCenter: parent.verticalCenter
                        size: 26
                    }
                }
                onClicked: root.newTerminalRequested()
            }
        }
        Arrival {
            Layout.fillWidth: true
            delay: 60
            spacing: 6

            PlainLabel {
                Layout.fillWidth: true
                text: qsTr("Start with a terminal")
                font.pixelSize: KodosiTheme.fontTitle
                font.weight: Font.DemiBold
                font.letterSpacing: -0.4
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }
            PlainLabel {
                Layout.fillWidth: true
                text: qsTr("It runs on this computer. Share it when you want company.")
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }
        }
        Arrival {
            Layout.alignment: Qt.AlignHCenter
            delay: 120

            RowLayout {
                spacing: 8

                KButton {
                    objectName: "stage.empty.folder"
                    Accessible.id: objectName
                    variant: KButton.Ghost
                    iconName: "folder"
                    text: qsTr("Open a folder…")
                    onClicked: Models.DesktopFiles.requestDirectory("new", Models.DesktopSettings.effectiveWorkingDirectory)
                }
                KButton {
                    objectName: "stage.empty.resume"
                    Accessible.id: objectName
                    variant: KButton.Ghost
                    iconName: "history"
                    text: qsTr("Resume a conversation")
                    onClicked: root.resumeRequested()
                }
            }
        }
    }
    KScrollView {
        id: scroll
        anchors.fill: parent
        visible: !root.loading && !root.failed && root.sessions.length > 0
        contentWidth: availableWidth

        ColumnLayout {
            x: (scroll.availableWidth - Math.min(1040, scroll.availableWidth)) / 2 + 36
            width: Math.min(1040, scroll.availableWidth) - 72
            spacing: 30

            Arrival {
                Layout.topMargin: 54
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

                Arrival {
                    id: place

                    required property var modelData
                    required property int index

                    Layout.fillWidth: true
                    delay: 40 * (index + 1)
                    spacing: 12

                    RowLayout {
                        spacing: 7
                        KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "laptop"; color: KodosiTheme.inkMuted }
                        PlainLabel { text: place.modelData.label; color: KodosiTheme.inkMuted; font.weight: Font.DemiBold; font.letterSpacing: -0.08 }
                    }
                    CardGrid {
                        id: grid
                        Layout.fillWidth: true
                        minimum: 230
                        maximum: 320

                        Repeater {
                            model: place.modelData.sessions

                            AbstractButton {
                                id: card

                                required property var modelData
                                readonly property bool needsYou: Models.Sessions.attention.indexOf(modelData.id) >= 0

                                width: grid.cardWidth
                                height: 68
                                hoverEnabled: true
                                activeFocusOnTab: true
                                objectName: "overview.session." + modelData.id
                                Accessible.id: objectName
                                Accessible.name: modelData.name
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

                                    AgentMark { Layout.leftMargin: 14; program: card.modelData.program || ""; size: 34; session: card.modelData }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 3
                                        PlainLabel { Layout.fillWidth: true; text: card.modelData.name; font.weight: Font.DemiBold; font.letterSpacing: -0.08; elide: Text.ElideRight }
                                        ActivityLine {
                                            Layout.fillWidth: true
                                            session: card.modelData
                                            words: card.modelData.activity || card.modelData.folderName || Identity.kindLabel(Identity.kind(card.modelData.program || ""))
                                        }
                                    }
                                    StatusSign { Layout.rightMargin: 14; session: card.modelData; unseen: card.needsYou; size: 13 }
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
                            height: 68
                            hoverEnabled: true
                            activeFocusOnTab: true
                            objectName: "overview.new"
                            Accessible.id: objectName
                            Accessible.name: qsTr("New terminal")
                            scale: pressed ? 0.98 : 1

                            Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                            background: DashedSlot {
                                radius: KodosiTheme.radiusLg
                                hovered: slot.hovered
                            }
                            contentItem: RowLayout {
                                spacing: 10
                                Prompt { Layout.leftMargin: 16; size: 15 }
                                PlainLabel { Layout.fillWidth: true; text: qsTr("New terminal"); color: KodosiTheme.inkMuted; font.weight: Font.DemiBold; font.letterSpacing: -0.08 }
                            }
                            onClicked: root.newTerminalRequested()
                        }
                    }
                }
            }
            Item { Layout.preferredHeight: 36 }
        }
    }
}
