pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

// A list inside the launcher's terminal, as a shell shows completions: comment
// lines name the groups, the copper cursor marks the line that is in use, and
// the line under the keys or the pointer shows in reverse. Typing narrows the
// list, Return takes a line and Escape closes the list.
FocusScope {
    id: root

    // Each line: { id, kind: "comment" | "choice" | "action", title, detail, current }.
    property var lines: []
    // The width that the list may take.
    property real room: 400
    property string filter: ""
    property string highlighted: ""
    property int shown: 0
    readonly property real rowHeight: 24
    readonly property real cell: metrics.advanceWidth("M")
    readonly property var matches: {
        const query = filter.trim().toLowerCase()
        if (query.length === 0)
            return lines
        const result = []
        let comment = null
        for (const line of lines) {
            if (line.kind === "comment") {
                comment = line
                continue
            }
            if ((line.title + " " + (line.detail || "")).toLowerCase().indexOf(query) < 0)
                continue
            if (comment) {
                result.push(comment)
                comment = null
            }
            result.push(line)
        }
        return result
    }
    readonly property int column: Math.max(0, ...matches.filter(line => line.kind !== "comment").map(line => line.title.length))
    readonly property int cells: Math.max(filter.length + 2, ...matches.map(line => {
        const detail = line.detail ? line.detail.length + 2 : 0
        return line.kind === "comment" ? line.title.length + 2 + detail : Math.max(column, line.title.length) + detail
    }))

    signal picked(string id)
    signal closed

    function move(step) {
        const choices = matches.filter(line => line.kind !== "comment")
        if (choices.length === 0)
            return
        let index = choices.findIndex(line => line.id === highlighted)
        if (index < 0)
            index = step > 0 ? -1 : 0
        highlighted = choices[(index + step + choices.length) % choices.length].id
        list.positionViewAtIndex(matches.findIndex(line => line.id === highlighted), ListView.Contain)
    }
    function pick() {
        if (matches.some(line => line.id === highlighted && line.kind !== "comment"))
            picked(highlighted)
    }

    implicitWidth: Math.min(room, 20 + cells * cell + 16)
    implicitHeight: (filter.length > 0 ? rowHeight : 0) + Math.min(Math.max(matches.length, 1), 9) * rowHeight
    activeFocusOnTab: true
    Accessible.role: Accessible.List

    Component.onCompleted: {
        const current = lines.find(line => line.current) || lines.find(line => line.kind !== "comment")
        highlighted = current ? current.id : ""
        if (KodosiTheme.reduceMotion)
            shown = 1000000
        else
            reveal.start()
        forceActiveFocus()
    }
    onFilterChanged: {
        const first = matches.find(line => line.kind !== "comment")
        highlighted = first ? first.id : ""
    }

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

    FontMetrics {
        id: metrics
        font.family: "monospace"
        font.pixelSize: KodosiTheme.fontBody
    }
    // The lines show one after another, as a terminal prints them.
    Timer {
        id: reveal
        interval: 14
        repeat: true
        onTriggered: {
            root.shown += 1
            if (root.shown >= root.lines.length) {
                root.shown = 1000000
                stop()
            }
        }
    }

    Row {
        id: filterLine

        visible: root.filter.length > 0
        x: 20
        height: root.rowHeight

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "❯ " + root.filter
            textFormat: Text.PlainText
            color: KodosiTheme.terminalInk
            font: metrics.font
        }
        CursorBlock {
            anchors.verticalCenter: parent.verticalCenter
            width: 8
            height: 15
            blinks: false
            color: KodosiTheme.terminalAccent
        }
    }
    ListView {
        id: list

        y: filterLine.visible ? root.rowHeight : 0
        width: root.width
        height: Math.min(Math.max(root.matches.length, 1), 9) * root.rowHeight
        clip: true
        interactive: root.matches.length > 9
        boundsBehavior: Flickable.StopAtBounds
        model: root.matches

        Text {
            visible: root.matches.length === 0
            x: 20
            height: root.rowHeight
            verticalAlignment: Text.AlignVCenter
            text: "# " + qsTr("nothing matches")
            color: KodosiTheme.terminalInkFaint
            font: metrics.font
        }

        delegate: Item {
            id: row

            required property var modelData
            required property int index
            readonly property bool comment: modelData.kind === "comment"
            readonly property bool lit: !comment && modelData.id === root.highlighted

            width: ListView.view.width
            height: root.rowHeight
            opacity: index < root.shown ? 1 : 0
            Accessible.role: comment ? Accessible.StaticText : Accessible.ListItem
            Accessible.name: [modelData.title, modelData.detail || ""].filter(part => part.length > 0).join(", ")
            Accessible.selected: modelData.current === true
            Accessible.onPressAction: {
                if (!row.comment)
                    root.picked(modelData.id)
            }

            Behavior on opacity { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }

            Rectangle {
                anchors.fill: parent
                anchors.rightMargin: 4
                radius: 3
                visible: row.lit
                color: KodosiTheme.terminalAccent
            }
            Rectangle {
                x: 7
                anchors.verticalCenter: parent.verticalCenter
                width: 6
                height: 15
                radius: 1.5
                visible: row.modelData.current === true
                color: row.lit ? KodosiTheme.terminal : KodosiTheme.terminalAccent
            }
            Row {
                x: 20
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 28
                clip: true

                Text {
                    text: row.comment ? "# " + row.modelData.title
                        : row.modelData.title + " ".repeat(Math.max(0, root.column - row.modelData.title.length))
                    textFormat: Text.PlainText
                    color: row.lit ? KodosiTheme.terminal : row.comment ? KodosiTheme.terminalInkMuted
                        : row.modelData.kind === "action" ? KodosiTheme.terminalAccent : KodosiTheme.terminalInk
                    font.family: metrics.font.family
                    font.pixelSize: metrics.font.pixelSize
                    font.weight: row.comment ? Font.Normal : Font.Medium
                }
                Text {
                    visible: !!row.modelData.detail
                    text: "  " + (row.modelData.detail || "")
                    textFormat: Text.PlainText
                    color: row.lit ? KodosiTheme.alpha(KodosiTheme.terminal, 0.75) : KodosiTheme.terminalInkFaint
                    font: metrics.font
                    elide: Text.ElideMiddle
                }
            }
            MouseArea {
                anchors.fill: parent
                enabled: !row.comment
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: root.highlighted = row.modelData.id
                onClicked: root.picked(row.modelData.id)
            }
        }
    }
}
