pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

// Pad the footer so the latest user message can scroll to the top of the viewport while the
// agent's reply grows underneath it.
Item {
    id: root

    signal tapped
    signal linkActivated(string link)

    required property Models.TerminalChat chat
    required property var session
    required property Look look
    required property string agent
    required property string program
    readonly property real column: Math.min(760, width - 28)
    readonly property real side: (width - column) / 2
    property bool following: true
    property bool endShown: true
    property real spacer: 0
    property bool settled: false

    function toEnd(animated: bool): void {
        following = true
        measure()
        const end = Math.max(list.originY, list.originY + list.contentHeight - list.height)
        if (animated && !KodosiTheme.reduceMotion) {
            latest.from = list.contentY
            latest.to = end
            latest.restart()
        } else {
            list.positionViewAtEnd()
        }
    }
    // Stop auto-scrolling when the user expands a step so the viewport stays anchored on it.
    function hold(): void {
        following = false
    }
    function measure(): void {
        const lines = root.chat.lines
        let newest = -1
        for (let index = lines.length - 1; index >= 0; --index) {
            if (lines[index].kind === "person") {
                newest = index
                break
            }
        }
        const last = list.count > 0 ? list.itemAtIndex(list.count - 1) : null
        const first = newest >= 0 ? list.itemAtIndex(newest) : null
        spacer = first && last ? Math.max(0, list.height - 18 - (last.y + last.height - first.y)) : 0
        endShown = !last || last.y + last.height <= list.contentY + list.height + 1
        if (following && !latest.running)
            list.positionViewAtEnd()
    }

    objectName: "chat.thread"
    Accessible.role: Accessible.List
    Accessible.name: agent

    Connections {
        function onThreadChanged() {
            Qt.callLater(root.measure)
            if (root.chat.loaded && !root.settled)
                Qt.callLater(() => root.settled = true)
        }

        target: root.chat
    }

    ListView {
        id: list

        anchors.fill: parent
        clip: true
        cacheBuffer: 2400
        boundsBehavior: Flickable.StopAtBounds
        model: Models.KeyedList { values: root.chat.lines }
        ScrollBar.vertical: KScrollBar {}

        TapHandler {
            onTapped: root.tapped()
        }

        onContentHeightChanged: Qt.callLater(root.measure)
        onHeightChanged: Qt.callLater(root.measure)
        onCountChanged: Qt.callLater(root.measure)
        onContentYChanged: {
            if (movingVertically && !atYEnd)
                root.following = false
            const last = count > 0 ? itemAtIndex(count - 1) : null
            root.endShown = !last || last.y + last.height <= contentY + height + 1
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
            onFinished: Qt.callLater(root.measure)
        }

        header: Item {
            width: 1
            height: 6
        }
        footer: Item {
            width: 1
            height: root.spacer + 12
        }

        delegate: Item {
            id: entry

            required property var modelData
            readonly property var line: modelData
            readonly property string kind: line.kind || ""
            readonly property bool mine: kind === "person"
                && (!line.byId || !Models.Account.signedIn || line.byId === Models.Account.userId)
            readonly property bool agents: kind === "answer" || kind === "plan" || kind === "work" || kind === "end"
                || kind === "now" || (kind === "note" && line.note !== "compacted")
            readonly property bool lead: agents && line.lead === true

            width: ListView.view.width
            height: body.y + body.height
            transformOrigin: Item.Bottom
            ListView.onAdd: {
                if (root.settled)
                    arrive.restart()
            }

            ParallelAnimation {
                id: arrive

                NumberAnimation { target: entry; property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade }
                NumberAnimation { target: entry; property: "scale"; from: 0.97; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
            }

            Loader {
                id: body

                x: root.side
                y: entry.kind === "day" ? 14 : entry.kind === "person" ? (entry.mine ? 18 : 16) : entry.agents ? (entry.lead ? 16 : 0) : 10
                width: root.column
                sourceComponent: entry.kind === "day" ? dayView
                    : entry.kind === "person" ? (entry.mine ? mineView : otherView)
                    : entry.agents ? agentView : compactedView
            }

            Component {
                id: dayView

                DayDivider {
                    date: new Date(entry.line.at)
                    look: root.look
                    height: implicitHeight + 4
                }
            }
            Component {
                id: mineView

                PersonBubble {
                    html: entry.line.html || ""
                    text: entry.line.text || ""
                    look: root.look
                    onLinkActivated: link => root.linkActivated(link)
                }
            }
            Component {
                id: otherView

                Item {
                    implicitHeight: Math.max(28, words.y + words.implicitHeight)
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("%1: %2").arg(entry.line.byName || "", entry.line.text || "")

                    PersonAvatar {
                        name: entry.line.byName || ""
                        key: entry.line.byId || ""
                        size: 28
                    }
                    Row {
                        id: who

                        x: 38
                        spacing: 6

                        PlainLabel {
                            id: otherName

                            text: entry.line.byName || ""
                            color: root.look.ink
                            font.weight: Font.DemiBold
                        }
                        PlainLabel {
                            anchors.baseline: otherName.baseline
                            visible: entry.line.at > 0
                            text: new Date(entry.line.at).toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
                            color: root.look.inkFaint
                            font.pixelSize: KodosiTheme.fontCaption2
                        }
                    }
                    TextEdit {
                        id: words

                        x: 38
                        y: who.height + 4
                        width: parent.width - 38
                        text: Prose.styled(entry.line.html || "", root.look)
                        textFormat: TextEdit.RichText
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        color: root.look.ink
                        font.pixelSize: KodosiTheme.fontBody
                        selectionColor: root.look.accent
                        selectedTextColor: root.look.accentInk
                        Accessible.ignored: true
                        onLinkActivated: link => root.linkActivated(link)
                    }
                }
            }
            Component {
                id: compactedView

                Item {
                    implicitHeight: KodosiTheme.scaled(22)

                    Raised {
                        anchors.centerIn: parent
                        width: compacted.implicitWidth + 20
                        height: KodosiTheme.scaled(22)
                        radius: height / 2
                        look: root.look
                        fill: root.look.raised

                        PlainLabel {
                            id: compacted

                            anchors.centerIn: parent
                            text: qsTr("compacted")
                            color: root.look.inkFaint
                            font.pixelSize: KodosiTheme.fontCaption
                            font.weight: Font.Medium
                        }
                    }
                }
            }
            Component {
                id: agentView

                Item {
                    implicitHeight: part.implicitHeight

                    AgentMark {
                        y: 2
                        visible: entry.lead
                        size: 26
                        program: root.program
                        account: root.session.programAccount ?? -1
                        working: entry.line.working === true
                    }
                    Column {
                        id: part

                        x: 36
                        width: parent.width - 36

                        Row {
                            visible: entry.lead
                            topPadding: 4
                            spacing: 6
                            Accessible.role: Accessible.Heading
                            Accessible.name: root.agent

                            PlainLabel {
                                id: name

                                text: root.agent
                                color: root.look.ink
                                font.weight: Font.DemiBold
                            }
                            PlainLabel {
                                anchors.baseline: name.baseline
                                text: (entry.kind === "now" ? new Date() : new Date(entry.line.at))
                                    .toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
                                visible: entry.kind === "now" || entry.line.at > 0
                                color: root.look.inkFaint
                                font.pixelSize: KodosiTheme.fontCaption2
                            }
                        }
                        Loader {
                            width: part.width
                            sourceComponent: entry.kind === "answer" ? answerView
                                : entry.kind === "plan" ? planView
                                : entry.kind === "work" ? workView
                                : entry.kind === "end" ? endView
                                : entry.kind === "now" ? nowView : noteView
                        }
                    }
                }
            }
            Component {
                id: answerView

                Item {
                    implicitHeight: prose.implicitHeight + 6

                    AgentProse {
                        id: prose

                        y: 6
                        width: parent.width
                        blocks: entry.line.blocks || []
                        live: entry.line.live === true
                        look: root.look
                        onLinkActivated: link => root.linkActivated(link)
                    }
                }
            }
            Component {
                id: planView

                Column {
                    topPadding: 8
                    spacing: 6

                    Row {
                        spacing: 5

                        KIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            width: KodosiTheme.scaled(11)
                            height: KodosiTheme.scaled(11)
                            name: "tasks"
                            strokeWidth: 2.2
                            color: root.look.inkMuted
                        }
                        PlainLabel {
                            text: qsTr("Plan")
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                            font.weight: Font.Medium
                        }
                    }
                    Item {
                        width: parent.width
                        implicitHeight: planProse.implicitHeight + 24

                        Raised {
                            anchors.fill: parent
                            radius: KodosiTheme.radiusLg
                            look: root.look
                            fill: root.look.raised
                        }
                        AgentProse {
                            id: planProse

                            x: 12
                            y: 12
                            width: parent.width - 24
                            blocks: entry.line.blocks || []
                            live: entry.line.live === true
                            look: root.look
                            onLinkActivated: link => root.linkActivated(link)
                        }
                    }
                }
            }
            Component {
                id: workView

                ChatWork {
                    topPadding: 4
                    line: entry.line
                    chat: root.chat
                    look: root.look
                    onHeld: root.hold()
                    onLinkActivated: link => root.linkActivated(link)
                }
            }
            Component {
                id: endView

                Row {
                    topPadding: 8
                    spacing: 8

                    Raised {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: entry.line.end !== "done"
                        width: endTag.implicitWidth + 18
                        height: KodosiTheme.scaled(20)
                        radius: height / 2
                        look: root.look
                        fill: root.look.raised

                        PlainLabel {
                            id: endTag

                            anchors.centerIn: parent
                            text: entry.line.end === "stopped" ? qsTr("Stopped") : entry.line.end === "failed" ? qsTr("Failed")
                                : entry.line.end === "refused" ? qsTr("Refused") : ""
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontCaption
                            font.weight: Font.Medium
                        }
                    }
                    PlainLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: text.length > 0
                        text: entry.line.worked || ""
                        color: root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                }
            }
            Component {
                id: noteView

                Row {
                    id: notice

                    readonly property bool recap: entry.line.note === "recap"

                    topPadding: 10
                    spacing: 8

                    KIcon {
                        width: KodosiTheme.scaled(13)
                        height: KodosiTheme.scaled(13)
                        y: 12
                        name: notice.recap ? "text" : entry.line.note === "signedOut" ? "key" : "warning"
                        strokeWidth: 2.4
                        color: notice.recap ? root.look.inkFaint : entry.line.note === "signedOut" ? root.look.accent : root.look.caution
                    }
                    Column {
                        width: parent.width - 21
                        spacing: notice.recap ? 3 : 4

                        KReadOnlyText {
                            width: parent.width
                            text: notice.recap ? qsTr("Recap")
                                : entry.line.note === "signedOut" ? qsTr("Needs you to sign in") : entry.line.error || ""
                            color: notice.recap ? root.look.inkMuted : root.look.ink
                            font.pixelSize: KodosiTheme.fontFootnote
                            font.weight: Font.DemiBold
                            selectionColor: root.look.accent
                        }
                        KReadOnlyText {
                            width: parent.width
                            visible: text.length > 0
                            text: entry.line.note === "signedOut" ? qsTr("Sign in with its own steps in the terminal.") : entry.line.text || ""
                            color: root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            selectionColor: root.look.accent
                        }
                    }
                }
            }
            Component {
                id: nowView

                Item {
                    implicitHeight: KodosiTheme.scaled(20) + (entry.lead ? 2 : 8)

                    RollingText {
                        y: entry.lead ? 2 : 8
                        width: Math.min(implicitWidth, parent.width)
                        height: KodosiTheme.scaled(20)
                        text: entry.line.words || ""
                        color: root.look.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                        elide: Text.ElideMiddle
                        objectName: "chat.live"
                    }
                }
            }
        }
    }
    AgentMark {
        anchors.centerIn: parent
        size: 40
        program: root.program
        account: root.session.programAccount ?? -1
        working: false
        opacity: root.chat.empty ? 1 : 0
        visible: opacity > 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
    }
    LatestKey {
        shown: !root.following && !root.endShown
        terminalLook: root.look
        objectName: "chat.latest"
        onClicked: root.toEnd(true)
    }
}
