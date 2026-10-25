pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

FocusScope {
    id: root

    property var rows: []
    property real listWidth: 300
    property string filter: ""
    property string highlighted: ""
    property bool hovering: false
    readonly property int visibleRows: 7
    readonly property real rowHeight: rows.some(row => !!row.subtitle) ? 44 : 32
    readonly property bool filters: rows.length > visibleRows || filter.length > 0
    readonly property var matches: {
        const query = filter.trim().toLowerCase()
        if (query.length === 0)
            return rows
        return rows.filter(row => row.kind !== "note"
            && [row.title, row.subtitle || "", row.detail || ""].join(" ").toLowerCase().indexOf(query) >= 0)
    }

    signal picked(string id)
    signal closed

    function reset() {
        filter = ""
        hovering = false
        const current = rows.find(row => row.current) || rows.find(row => row.kind !== "note")
        highlighted = current ? current.id : ""
        forceActiveFocus()
    }
    function move(step) {
        const choices = matches.filter(row => row.kind !== "note")
        if (choices.length === 0)
            return
        let index = choices.findIndex(row => row.id === highlighted)
        if (index < 0)
            index = step > 0 ? -1 : 0
        hovering = false
        highlighted = choices[(index + step + choices.length) % choices.length].id
    }
    function sync() {
        list.currentIndex = matches.findIndex(row => row.id === highlighted && row.kind !== "note")
    }
    function pick() {
        if (matches.some(row => row.id === highlighted && row.kind !== "note"))
            picked(highlighted)
    }

    implicitWidth: listWidth
    implicitHeight: (filters ? 36 : 0) + Math.min(Math.max(matches.length, 1), visibleRows) * (rowHeight + 2) + 10
    Accessible.role: Accessible.List

    onFilterChanged: {
        const first = matches.find(row => row.kind !== "note")
        hovering = false
        highlighted = first ? first.id : ""
    }
    onHighlightedChanged: sync()
    onMatchesChanged: sync()

    Keys.onUpPressed: move(-1)
    Keys.onDownPressed: move(1)
    Keys.onReturnPressed: pick()
    Keys.onEnterPressed: pick()
    Keys.onEscapePressed: {
        if (filter.length > 0)
            filter = ""
        else
            closed()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Backspace && filter.length > 0) {
            filter = filter.slice(0, -1)
            event.accepted = true
        } else if (event.text.length === 1 && event.text >= " " && event.text !== "\u007f"
                && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
                && (event.text !== " " || filter.length > 0)) {
            filter += event.text
            event.accepted = true
        }
    }

    Well {
        id: filterWell

        visible: root.filters
        x: 8
        y: 8
        width: root.width - 16
        height: 28
        radius: KodosiTheme.radiusSm

        Row {
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 7

            KIcon { anchors.verticalCenter: parent.verticalCenter; width: 12; height: 12; name: "search"; color: KodosiTheme.inkFaint; strokeWidth: 2.2 }
            PlainLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: root.filter.length > 0 ? root.filter : qsTr("Type to find")
                color: root.filter.length > 0 ? KodosiTheme.ink : KodosiTheme.inkFaint
                font.pixelSize: KodosiTheme.fontFootnote
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.filter.length > 0
                width: 2
                height: 14
                radius: 1
                color: KodosiTheme.accent
            }
        }
    }
    ListView {
        id: list

        x: 6
        y: (root.filters ? 36 : 0) + 5
        width: root.width - 12
        height: Math.min(Math.max(root.matches.length, 1), root.visibleRows) * (root.rowHeight + 2)
        spacing: 2
        clip: true
        interactive: root.matches.length > root.visibleRows
        boundsBehavior: Flickable.StopAtBounds
        model: root.matches
        currentIndex: -1
        highlightFollowsCurrentItem: true
        highlightMoveDuration: root.hovering ? KodosiTheme.motionSnappy : 0
        highlightResizeDuration: 0
        highlight: Raised {
            radius: KodosiTheme.radiusSm
            fill: KodosiTheme.lifted

            Rectangle {
                x: -1.5
                anchors.verticalCenter: parent.verticalCenter
                width: 3
                height: 14
                radius: 1.5
                color: KodosiTheme.accent
            }
        }

        PlainLabel {
            visible: root.matches.length === 0
            width: list.width
            height: root.rowHeight
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            text: qsTr("Nothing matches")
            color: KodosiTheme.inkFaint
            font.pixelSize: KodosiTheme.fontFootnote
        }

        delegate: Item {
            id: row

            required property var modelData
            required property int index
            readonly property bool note: modelData.kind === "note"

            width: ListView.view.width
            height: root.rowHeight
            Accessible.role: note ? Accessible.StaticText : Accessible.ListItem
            Accessible.name: [modelData.title, modelData.subtitle || "", modelData.detail || "", modelData.usage ? modelData.usage.spoken : ""]
                .filter(part => part.length > 0).join(", ")
            Accessible.selected: modelData.current === true
            Accessible.onPressAction: {
                if (!row.note)
                    root.picked(modelData.id)
            }

            Row {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10

                IconTile {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: row.modelData.icon === "folder"
                    iconName: "folder"
                    tint: "#4f9bb0"
                    size: 26
                }
                IconTile {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: row.modelData.icon === "device"
                    iconName: "laptop"
                    tint: "#6b6560"
                    size: 26
                }
                AccountMark {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: row.modelData.icon === "account"
                    initial: row.modelData.initial || ""
                    key: row.modelData.key || ""
                    signedIn: row.modelData.signedIn === true
                    usage: row.modelData.usage ?? null
                    size: 26
                }
                SlotMark {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: row.modelData.icon === "slot"
                    glyph: row.modelData.glyph || ""
                    round: row.modelData.glyph !== "folder"
                    size: 26
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: list.width - 20 - (row.modelData.icon ? 36 : 0) - (row.modelData.current ? 22 : 0) - (detail.visible ? detail.width + 8 : 0)
                        - (percent.visible ? percent.width + 8 : 0)
                    spacing: 1

                    ShimmerText {
                        width: parent.width
                        text: row.modelData.title
                        active: row.modelData.working === true
                        color: row.modelData.kind === "action" ? KodosiTheme.accentStrong : row.note ? KodosiTheme.inkFaint : KodosiTheme.ink
                        font.pixelSize: KodosiTheme.fontBody
                        font.weight: Font.Medium
                        elide: Text.ElideRight
                    }
                    PlainLabel {
                        width: parent.width
                        visible: !!row.modelData.subtitle
                        text: row.modelData.subtitle || ""
                        color: KodosiTheme.inkMuted
                        font.family: row.modelData.icon === "folder" ? "monospace" : Application.font.family
                        font.pixelSize: KodosiTheme.fontFootnote
                        elide: Text.ElideMiddle
                    }
                }
            }
            PlainLabel {
                id: percent

                readonly property string level: row.modelData.usage ? row.modelData.usage.level : ""

                anchors.right: check.visible ? check.left : parent.right
                anchors.rightMargin: check.visible ? 8 : 10
                anchors.verticalCenter: parent.verticalCenter
                visible: !!row.modelData.usage
                text: row.modelData.usage ? row.modelData.usage.used + "%" : ""
                color: level === "reached" ? KodosiTheme.danger : level === "near" ? KodosiTheme.caution : KodosiTheme.inkFaint
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
            }
            PlainLabel {
                id: detail

                anchors.right: check.visible ? check.left : parent.right
                anchors.rightMargin: check.visible ? 8 : 10
                anchors.verticalCenter: parent.verticalCenter
                visible: !!row.modelData.detail
                width: Math.min(implicitWidth, 130)
                text: row.modelData.detail || ""
                color: KodosiTheme.inkFaint
                font.family: "monospace"
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideMiddle
            }
            KIcon {
                id: check

                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                visible: row.modelData.current === true
                width: 13
                height: 13
                name: "check"
                color: KodosiTheme.accentStrong
                strokeWidth: 2.6
            }
            MouseArea {
                anchors.fill: parent
                enabled: !row.note
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: {
                    root.hovering = true
                    root.highlighted = row.modelData.id
                }
                onClicked: root.picked(row.modelData.id)
            }
        }
    }
}
