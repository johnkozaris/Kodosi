pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property bool listening: false
    property bool following: true
    readonly property var details: Models.Dosi.details

    signal settingsRequested

    function prose(text) {
        let html = Mentions.escaped(text)
        html = html.replace(/`([^`\n]+)`/g, "<code>$1</code>")
        html = html.replace(/\*\*([^*\n]+)\*\*/g, "<b>$1</b>")
        html = html.replace(/(^|[^*\w])\*([^*\n]+)\*(?![\w*])/g, "$1<i>$2</i>")
        html = html.replace(/\[([^\]\n]+)\]\((https:\/\/[^\s)&]+)\)/g, "<a href=\"$2\">$1</a>")
        html = html.replace(/(^|[\s(])(https:\/\/[^\s<)&]+)/g, "$1<a href=\"$2\">$2</a>")
        return "<span style=\"white-space:pre-wrap\">" + html + "</span>"
    }
    function openLink(link) {
        if (link.startsWith("https://"))
            Models.DesktopFiles.openWebUrl(link)
    }
    function toEnd() {
        if (root.following)
            list.positionViewAtEnd()
    }
    function showLatest() {
        root.following = true
        latest.from = list.contentY
        latest.to = Math.max(list.originY - list.topMargin, list.originY + list.contentHeight + list.bottomMargin - list.height)
        latest.restart()
    }

    objectName: "dosi.thread"
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Conversation with Dosi")

    Connections {
        function onEmptyChanged() {
            if (Models.Dosi.empty)
                root.following = true
        }

        target: Models.Dosi
    }

    ListView {
        id: list

        anchors.fill: parent
        clip: true
        topMargin: Math.max(10, height - contentHeight - bottomMargin)
        bottomMargin: 8
        leftMargin: 14
        rightMargin: 14
        boundsBehavior: Flickable.StopAtBounds
        model: Models.KeyedList { values: Models.Dosi.rows }
        ScrollBar.vertical: KScrollBar {}

        onContentHeightChanged: Qt.callLater(root.toEnd)
        onHeightChanged: Qt.callLater(root.toEnd)
        onCountChanged: Qt.callLater(root.toEnd)
        onMovingVerticallyChanged: {
            if (movingVertically && !atYEnd)
                root.following = false
        }
        onContentYChanged: {
            if (movingVertically && !atYEnd)
                root.following = false
        }
        onAtYEndChanged: {
            if (atYEnd)
                root.following = true
        }

        NumberAnimation {
            id: latest
            target: list
            property: "contentY"
            duration: KodosiTheme.motionSoft
            easing.type: Easing.OutCubic
        }

        header: Item {
            width: ListView.view.width - 28
            height: root.details.earlier ? 40 : 0
            visible: root.details.earlier === true

            KButton {
                anchors.centerIn: parent
                compact: true
                variant: KButton.Ghost
                iconName: "chevron-up"
                text: qsTr("Earlier")
                enabled: root.details.loadingEarlier !== true
                objectName: "dosi.earlier"
                Accessible.id: objectName
                onClicked: {
                    root.following = false
                    Models.Dosi.loadEarlier()
                }
            }
        }
        footer: Item {
            width: ListView.view.width - 28
            height: away.visible ? 28 : 0

            Row {
                id: away
                x: 28
                anchors.verticalCenter: parent.verticalCenter
                spacing: 7
                visible: (root.details.remote || "").length > 0 && Models.Dosi.state === "working"

                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 11
                    height: 11
                    name: "laptop"
                    color: KodosiTheme.inkFaint
                }
                ShimmerText {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Working on %1").arg(root.details.remote || "")
                    color: KodosiTheme.inkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                }
            }
        }
        add: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade }
            NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
        }
        displaced: Transition {
            NumberAnimation { property: "y"; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
        }

        delegate: Item {
            id: entry

            required property var modelData
            readonly property var row: modelData
            readonly property string kind: row.kind || ""
            readonly property real gap: kind === "divider" ? 14 : kind === "person" ? 12 : row.starts ? 12 : 6

            width: ListView.view.width - 28
            height: gap + body.height
            transformOrigin: Item.Bottom

            Loader {
                id: body
                y: entry.gap
                width: entry.width
                sourceComponent: entry.kind === "divider" ? dividerView : entry.kind === "person" ? personView : turnView
            }
            Component {
                id: dividerView
                Item {
                    height: 16

                    Rectangle {
                        anchors.left: parent.left
                        anchors.right: label.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        height: 1
                        color: KodosiTheme.alpha(KodosiTheme.hairline, 0.45)
                    }
                    PlainLabel {
                        id: label
                        anchors.centerIn: parent
                        text: entry.row.label || ""
                        color: KodosiTheme.inkFaint
                        font.family: "monospace"
                        font.pixelSize: KodosiTheme.fontCaption2
                        font.letterSpacing: 0.4
                    }
                    Rectangle {
                        anchors.left: label.right
                        anchors.leftMargin: 8
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        height: 1
                        color: KodosiTheme.alpha(KodosiTheme.hairline, 0.45)
                    }
                }
            }
            Component {
                id: personView
                Item {
                    height: bubble.height

                    Rectangle {
                        id: bubble
                        anchors.right: parent.right
                        width: Math.min(words.implicitWidth, parent.width - 48 - 24) + 24
                        height: words.implicitHeight + 16
                        topLeftRadius: 14
                        topRightRadius: 14
                        bottomLeftRadius: 14
                        bottomRightRadius: 4
                        color: KodosiTheme.accentSoft

                        TextEdit {
                            id: words
                            x: 12
                            y: 8
                            width: Math.min(implicitWidth, bubble.parent.width - 48 - 24)
                            text: root.prose(entry.row.text || "")
                            textFormat: TextEdit.RichText
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.Wrap
                            color: KodosiTheme.ink
                            font.pixelSize: KodosiTheme.fontBody
                            selectionColor: KodosiTheme.accent
                            selectedTextColor: KodosiTheme.accentInk
                            Accessible.name: entry.row.text || ""
                            onLinkActivated: link => root.openLink(link)

                            HoverHandler { cursorShape: words.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor }
                        }
                    }
                }
            }
            Component {
                id: turnView
                Item {
                    height: content.height

                    DosiFace {
                        visible: entry.row.starts === true
                        size: 20
                        mood: entry.row.turn === Models.Dosi.liveTurn ? Models.Dosi.mood : "resting"
                        still: true
                    }
                    Loader {
                        id: content
                        x: 28
                        width: parent.width - 28
                        sourceComponent: entry.kind === "text" ? proseView
                            : entry.kind === "reasoning" ? thoughtView
                            : entry.kind === "tool" || entry.kind === "tools" ? stepsView
                            : entry.kind === "question" ? questionView
                            : entry.kind === "notice" ? noticeView : noteView
                    }
                }
            }
            Component {
                id: proseView
                Item {
                    height: Math.max(prose.implicitHeight, 18)

                    TextEdit {
                        id: prose
                        width: Math.min(implicitWidth, parent.width - (entry.row.live ? 12 : 0))
                        text: root.prose(entry.row.text || "")
                        textFormat: TextEdit.RichText
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        color: KodosiTheme.ink
                        font.pixelSize: KodosiTheme.fontBody
                        selectionColor: KodosiTheme.accent
                        selectedTextColor: KodosiTheme.accentInk
                        Accessible.name: entry.row.text || ""
                        onLinkActivated: link => root.openLink(link)

                        HoverHandler { cursorShape: prose.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor }
                    }
                    CursorBlock {
                        x: Math.min(prose.positionToRectangle(prose.length).x + 3, parent.width - width)
                        y: prose.positionToRectangle(prose.length).y + 2
                        width: 6
                        height: 14
                        blinks: true
                        visible: entry.row.live === true
                    }
                }
            }
            Component {
                id: thoughtView
                Column {
                    id: thinking

                    property bool open: false

                    spacing: 5

                    AbstractButton {
                        id: toggle
                        implicitWidth: toggleRow.implicitWidth
                        implicitHeight: 18
                        enabled: (entry.row.text || "").length > 0
                        onClicked: thinking.open = !thinking.open

                        contentItem: Row {
                            id: toggleRow
                            spacing: 6

                            KIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 8
                                height: 8
                                name: "chevron-down"
                                rotation: thinking.open ? 0 : -90
                                strokeWidth: 2.8
                                color: KodosiTheme.inkFaint

                                Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                            }
                            ShimmerText {
                                anchors.verticalCenter: parent.verticalCenter
                                text: entry.row.live ? qsTr("thinking")
                                    : entry.row.seconds >= 0 ? qsTr("thought for %1s").arg(entry.row.seconds) : qsTr("thought")
                                color: KodosiTheme.inkFaint
                                font.family: "monospace"
                                font.pixelSize: KodosiTheme.fontCaption
                            }
                        }
                    }
                    Item {
                        visible: thinking.open
                        width: thinking.width
                        height: thoughtText.implicitHeight

                        Rectangle {
                            x: 4
                            width: 1
                            height: parent.height
                            color: KodosiTheme.lifted
                        }
                        KReadOnlyText {
                            id: thoughtText
                            x: 16
                            width: parent.width - 16
                            text: entry.row.text || ""
                            color: KodosiTheme.inkFaint
                            font.pixelSize: KodosiTheme.fontFootnote
                        }
                    }
                }
            }
            Component {
                id: stepsView
                DosiSteps {
                    row: entry.row
                }
            }
            Component {
                id: questionView
                DosiQuestion {
                    row: entry.row
                }
            }
            Component {
                id: noteView
                KReadOnlyText {
                    text: entry.row.text || ""
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
            }
            Component {
                id: noticeView
                Item {
                    readonly property bool stopped: entry.row.tone === "stopped"

                    height: stopped ? 18 : card.height

                    Rectangle {
                        visible: parent.stopped
                        width: stoppedLabel.implicitWidth + 16
                        height: 18
                        radius: 9
                        color: KodosiTheme.alpha(KodosiTheme.ink, 0.06)

                        PlainLabel {
                            id: stoppedLabel
                            anchors.centerIn: parent
                            text: qsTr("Stopped")
                            color: KodosiTheme.inkFaint
                            font.family: "monospace"
                            font.pixelSize: KodosiTheme.fontCaption2
                        }
                    }
                    Item {
                        id: card
                        visible: !parent.stopped
                        width: parent.width
                        height: cardColumn.implicitHeight + 22

                        Raised {
                            anchors.fill: parent
                            radius: KodosiTheme.radiusLg
                            fill: KodosiTheme.raised
                        }
                        Column {
                            id: cardColumn
                            x: 12
                            y: 11
                            width: parent.width - 24
                            spacing: 7

                            Row {
                                spacing: 8

                                KIcon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 13
                                    height: 13
                                    name: entry.row.tone === "signIn" ? "key" : "warning"
                                    strokeWidth: 2.2
                                    color: KodosiTheme.caution
                                }
                                PlainLabel {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: entry.row.tone === "signIn" ? qsTr("Dosi needs you to sign in") : qsTr("Dosi stopped")
                                    color: KodosiTheme.ink
                                    font.pixelSize: KodosiTheme.fontFootnote
                                    font.weight: Font.DemiBold
                                }
                            }
                            KReadOnlyText {
                                width: parent.width
                                visible: (entry.row.text || "").length > 0
                                text: entry.row.text || ""
                                color: KodosiTheme.inkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                            }
                            KButton {
                                visible: entry.row.tone === "signIn"
                                compact: true
                                text: qsTr("Open Settings")
                                onClicked: root.settingsRequested()
                            }
                        }
                    }
                }
            }
        }
    }
    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 300)
        spacing: 14
        visible: Models.Dosi.empty
        opacity: welcome.progress
        transform: Arrive { id: welcome; rise: 8 }
        onVisibleChanged: {
            if (visible)
                welcome.play()
        }

        DosiFace {
            anchors.horizontalCenter: parent.horizontalCenter
            size: 58
            mood: Models.Dosi.mood === "resting" && root.listening ? "listening" : Models.Dosi.mood
        }
        PlainLabel {
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Your agent for your agents")
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontCallout
        }
        Column {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 6

            Repeater {
                model: [qsTr("What needs me?"), qsTr("Sum up my rooms"), qsTr("Tell me when my terminals finish")]

                KButton {
                    id: ask

                    required property string modelData
                    required property int index

                    anchors.horizontalCenter: parent.horizontalCenter
                    compact: true
                    text: modelData
                    enabled: Models.Dosi.state !== "unavailable"
                    opacity: askArrive.progress
                    transform: Arrive { id: askArrive; delay: 200 + ask.index * 60; rise: 6 }
                    onVisibleChanged: {
                        if (visible)
                            askArrive.play()
                    }
                    Component.onCompleted: askArrive.play()
                    onClicked: Models.Dosi.send(modelData)
                }
            }
        }
    }
    KButton {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.following ? -height : 8
        compact: true
        iconName: "chevron-down"
        text: qsTr("Latest")
        opacity: root.following ? 0 : 1
        visible: opacity > 0
        objectName: "dosi.latest"
        Accessible.id: objectName
        onClicked: root.showLatest()

        Behavior on anchors.bottomMargin { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
    }
}
