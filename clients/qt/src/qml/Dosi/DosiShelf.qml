pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property var terminals: {
        const ranked = Models.Sessions.listedSessions.map((session, index) => ({ session: session, index: index, need: need(session) }))
        ranked.sort((a, b) => a.need - b.need || a.index - b.index)
        return ranked.map(entry => entry.session)
    }
    readonly property var rooms: {
        const unread = Models.Missions.unreadRooms
        const ranked = Models.Missions.missions.map((room, index) => ({ room: room, index: index, unread: unread.indexOf(room.id) >= 0 }))
        ranked.sort((a, b) => (b.unread ? 1 : 0) - (a.unread ? 1 : 0) || a.index - b.index)
        return ranked.map(entry => entry.room)
    }

    signal newTerminalRequested
    signal newRoomRequested

    function form(session) {
        const unseen = Models.Sessions.attention.indexOf(session.id) >= 0
        return session.sign ? (session.asks || unseen ? session.sign : "") : unseen ? "changed" : ""
    }
    function need(session) {
        const sign = form(session)
        if (sign === "hand" || sign === "question" || sign === "key")
            return 0
        if (sign === "failed")
            return 1
        if (sign === "done" || sign === "changed")
            return 2
        return session.working === true ? 3 : 4
    }
    function unread(room) {
        return Models.Missions.unreadRooms.indexOf(room.id) >= 0
    }
    function chipWidth(name, sign) {
        return 37 + Math.min(Math.ceil(metrics.advanceWidth(name || "")), 112) + (sign > 0 ? 6 + sign : 0)
    }
    function moreWidth(hidden) {
        return 28 + Math.ceil(metrics.advanceWidth(String(hidden)))
    }
    function shown(widths, room) {
        let total = 0
        for (let index = 0; index < widths.length; ++index)
            total += widths[index] + (index > 0 ? 4 : 0)
        if (total <= room)
            return widths.length
        let used = 0
        let count = 0
        while (count < widths.length && used + widths[count] + 4 + moreWidth(widths.length - count - 1) <= room) {
            used += widths[count] + 4
            count += 1
        }
        return count
    }

    implicitHeight: column.implicitHeight + 10
    objectName: "dosi.shelf"

    Behavior on implicitHeight { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }

    PlainLabel {
        id: probe
        visible: false
        font.pixelSize: KodosiTheme.fontCaption
        font.weight: Font.DemiBold
    }
    FontMetrics {
        id: metrics
        font: probe.font
    }
    Raised {
        anchors.fill: parent
        radius: KodosiTheme.radiusLg
        fill: KodosiTheme.surface
        opacity: well.progress
        transform: Arrive { id: well; delay: 80; rise: 0 }
        onVisibleChanged: {
            if (visible)
                well.play()
        }
    }
    Column {
        id: column
        x: 5
        y: 5
        width: parent.width - 10
        spacing: 4

        ShelfRow {
            width: column.width
            values: root.terminals
            widths: root.terminals.map(session => root.chipWidth(session.name, root.form(session).length > 0 ? 14 : 0))
            count: root.shown(widths, width - 34)
            needs: hidden.some(session => root.need(session) <= 1)
            addName: qsTr("New terminal")
            addIdentifier: "dosi.marks.new"
            addsTerminal: true
            onAdd: root.newTerminalRequested()
            chip: Component {
                TerminalChip {}
            }
        }
        ShelfRow {
            width: column.width
            visible: root.rooms.length > 0
            values: root.rooms
            widths: root.rooms.map(room => root.chipWidth(room.name, root.unread(room) ? 8 : 0))
            count: root.shown(widths, width - 34)
            needs: hidden.some(room => root.unread(room))
            rooms: true
            addName: qsTr("New room")
            addIdentifier: "dosi.marks.room.new"
            onAdd: root.newRoomRequested()
            chip: Component {
                RoomChip {}
            }
        }
    }

    component ShelfRow: Item {
        id: shelf

        property var values: []
        property var widths: []
        property int count: 0
        property bool needs: false
        property bool rooms: false
        property string addName: ""
        property string addIdentifier: ""
        property bool addsTerminal: false
        property Component chip
        readonly property var hidden: values.slice(count)
        readonly property var chips: values.slice(0, count).map((value, index) => ({ id: value.id, value: value, width: widths[index] }))
        readonly property real listWidth: chips.reduce((total, entry) => total + entry.width, 0) + 4 * Math.max(chips.length - 1, 0)

        signal add

        height: 30

        ListView {
            id: list
            x: 2
            width: shelf.listWidth
            height: parent.height
            orientation: ListView.Horizontal
            spacing: 4
            interactive: false
            opacity: names.progress
            transform: Arrive { id: names; delay: 100; rise: 0 }
            onVisibleChanged: {
                if (visible)
                    names.play()
            }
            model: Models.KeyedList { values: shelf.chips }
            delegate: shelf.chip
            add: Transition {
                NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
            }
            remove: Transition {
                NumberAnimation { property: "scale"; to: 0.9; duration: KodosiTheme.motionFade; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
                NumberAnimation { property: "opacity"; to: 0; duration: KodosiTheme.motionFade; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
            }
            displaced: Transition {
                NumberAnimation { property: "x"; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
            }
            move: Transition {
                NumberAnimation { property: "x"; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
            }
        }
        MoreChip {
            x: list.x + list.width + (shelf.count > 0 ? 4 : 0)
            y: 2
            visible: opacity > 0
            opacity: shelf.hidden.length > 0 ? names.progress : 0
            values: shelf.hidden
            rooms: shelf.rooms
            needs: shelf.needs
            order: shelf.count

            Behavior on x { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }
        }
        AbstractButton {
            id: plus

            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: 26
            height: 26
            hoverEnabled: true
            activeFocusOnTab: true
            objectName: shelf.addIdentifier
            Accessible.id: objectName
            Accessible.name: shelf.addName
            ToolTip.visible: hovered
            ToolTip.text: shelf.addName
            ToolTip.delay: Tip.delay
            scale: pressed ? 0.95 : 1
            onClicked: shelf.add()

            Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

            background: DashedSlot {
                radius: KodosiTheme.radiusSm
                hovered: plus.hovered
                opacity: spot.hidden ? 0 : 1

                FoldSpot {
                    id: spot
                    anchors.fill: parent
                    key: shelf.addsTerminal ? "new" : "new.room"
                    place: "slot"
                    fill: KodosiTheme.alpha(KodosiTheme.accent, 0.12)
                    radius: KodosiTheme.radiusSm
                }
            }
            contentItem: Item {
                KIcon {
                    anchors.centerIn: parent
                    width: 12
                    height: 12
                    name: "plus"
                    strokeWidth: 2.6
                    color: plus.hovered ? KodosiTheme.accentStrong : KodosiTheme.inkFaint
                }
            }

            KFocusIndicator { active: plus.visualFocus }
        }
    }

    component MoreChip: AbstractButton {
        id: moreButton

        property var values: []
        property bool rooms: false
        property bool needs: false
        property int order: 0
        property int glints: 0
        property int glintDelay: 0
        readonly property string readingItem: Models.Dosi.reading.item || ""
        readonly property bool open: morePopover.opened
        readonly property bool selected: values.some(value => rooms
            ? Models.DesktopState.activeView === Models.DesktopState.Missions && Models.Missions.selectedMissionId === value.id
            : Models.DesktopState.activeView === Models.DesktopState.Sessions && Models.DesktopState.selectedSessionId === value.id)
        readonly property color ink: needs ? KodosiTheme.accentStrong : open || hovered ? KodosiTheme.ink : KodosiTheme.inkMuted

        implicitWidth: 28 + Math.ceil(moreMetrics.advanceWidth(moreLabel.text))
        implicitHeight: 26
        hoverEnabled: true
        activeFocusOnTab: true
        objectName: rooms ? "dosi.marks.room.more" : "dosi.marks.more"
        Accessible.id: objectName
        Accessible.name: rooms ? qsTr("%n more rooms", "", values.length) : qsTr("%n more terminals", "", values.length)
        ToolTip.visible: hovered && !open
        ToolTip.text: Accessible.name
        ToolTip.delay: Tip.delay
        scale: pressed ? 0.95 : 1
        onClicked: morePopover.opened ? morePopover.close() : morePopover.open()
        onValuesChanged: {
            if (values.length === 0)
                morePopover.close()
        }
        onReadingItemChanged: {
            const reading = Models.Dosi.reading
            if (reading.kind === (rooms ? "rooms" : "terminals")) {
                glintDelay = Math.min(order, 10) * 70
                glints += 1
            } else if (!rooms && reading.kind === "terminal" && values.some(value => value.id === reading.sessionId)) {
                glintDelay = 0
                glints += 1
            } else if (rooms && reading.kind === "room" && values.some(value => value.id === reading.roomId)) {
                glintDelay = 0
                glints += 1
            }
        }

        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

        FontMetrics {
            id: moreMetrics
            font: moreLabel.font
        }

        background: Item {
            Bezel {
                anchors.fill: parent
                radius: KodosiTheme.radiusSm
                quiet: true
                pressed: moreButton.pressed
                fill: moreButton.open || moreButton.selected ? KodosiTheme.accentSoft
                    : KodosiTheme.mix(KodosiTheme.raised, KodosiTheme.ink, moreButton.hovered ? 0.05 : 0)
            }
            Cue {
                anchors.fill: parent
                radius: KodosiTheme.radiusSm
                glints: moreButton.glints
                delay: moreButton.glintDelay
            }
            Repeater {
                model: moreButton.values

                FoldSpot {
                    required property var modelData

                    anchors.centerIn: parent
                    width: 20
                    height: 20
                    key: (moreButton.rooms ? "room:" : "terminal:") + (modelData.id || "")
                    place: "slot"
                    session: moreButton.rooms ? null : modelData
                    sigil: moreButton.rooms ? modelData.id || "" : ""
                    tucked: true
                }
            }
        }
        contentItem: Item {
            Row {
                anchors.centerIn: parent
                spacing: 4

                PlainLabel {
                    id: moreLabel
                    anchors.verticalCenter: parent.verticalCenter
                    text: String(moreButton.values.length)
                    color: moreButton.ink
                    font.pixelSize: KodosiTheme.fontCaption
                    font.weight: Font.DemiBold
                }
                KIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8
                    height: 8
                    name: "chevron-down"
                    strokeWidth: 2.6
                    color: moreButton.ink
                    rotation: moreButton.open ? 180 : 0

                    Behavior on rotation { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                }
            }
        }

        KFocusIndicator { active: moreButton.visualFocus }

        KPopover {
            id: morePopover

            y: moreButton.height + 6
            width: 240
            height: Math.min(moreButton.values.length * 30 + 10, 312)
            radius: KodosiTheme.radiusLg
            transformOrigin: Popup.TopLeft
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
            objectName: moreButton.objectName + ".list"

            contentItem: Flickable {
                clip: true
                contentHeight: moreColumn.implicitHeight + 10
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                Column {
                    id: moreColumn
                    x: 6
                    y: 5
                    width: parent.width - 12
                    spacing: 2

                    Repeater {
                        model: moreButton.rooms ? [] : moreButton.values

                        MoreRow {
                            id: terminalRow

                            required property var modelData
                            required property int index

                            order: index
                            name: modelData.name || ""
                            detail: modelData.kind === "remote" ? modelData.hostName || "" : ""
                            selected: Models.DesktopState.activeView === Models.DesktopState.Sessions
                                && Models.DesktopState.selectedSessionId === modelData.id
                            objectName: "dosi.marks.more." + (modelData.id || "")
                            Accessible.id: objectName
                            onClicked: {
                                morePopover.close()
                                Models.SessionActions.activate(terminalRow.modelData.id)
                            }
                            mark: AgentMark {
                                size: 18
                                program: terminalRow.modelData.program || ""
                                session: terminalRow.modelData
                            }
                            sign: StatusSign {
                                size: 9
                                session: terminalRow.modelData
                                unseen: Models.Sessions.attention.indexOf(terminalRow.modelData.id) >= 0
                            }
                        }
                    }
                    Repeater {
                        model: moreButton.rooms ? moreButton.values : []

                        MoreRow {
                            id: roomRow

                            required property var modelData
                            required property int index

                            order: index
                            name: modelData.name || ""
                            selected: Models.DesktopState.activeView === Models.DesktopState.Missions
                                && Models.Missions.selectedMissionId === modelData.id
                            objectName: "dosi.marks.room.more." + (modelData.id || "")
                            Accessible.id: objectName
                            onClicked: {
                                morePopover.close()
                                Models.DesktopState.activeView = Models.DesktopState.Missions
                                Models.Missions.open(roomRow.modelData.id)
                            }
                            mark: RoomSigil {
                                size: 18
                                key: roomRow.modelData.id || ""
                            }
                            sign: NewDot {
                                visible: Models.Missions.unreadRooms.indexOf(roomRow.modelData.id) >= 0
                            }
                        }
                    }
                }
            }
        }
    }

    component MoreRow: AbstractButton {
        id: moreRow

        property int order: 0
        property string name: ""
        property string detail: ""
        property bool selected: false
        property alias mark: rowMark.data
        property alias sign: rowSign.data

        width: parent ? parent.width : 0
        height: 28
        hoverEnabled: true
        Accessible.name: name
        Accessible.selected: selected
        opacity: rowArrive.progress
        transform: Arrive { id: rowArrive; delay: Math.min(moreRow.order, 10) * 25; rise: 6 }
        onVisibleChanged: {
            if (visible)
                rowArrive.play()
        }

        background: Rectangle {
            radius: KodosiTheme.radiusSm
            color: moreRow.selected ? KodosiTheme.accentSoft
                : moreRow.hovered ? KodosiTheme.mix(KodosiTheme.lifted, KodosiTheme.ink, 0.06) : "transparent"

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
        }
        contentItem: Item {
            Item {
                id: rowMark
                x: 7
                width: 18
                height: 18
                anchors.verticalCenter: parent.verticalCenter
            }
            PlainLabel {
                x: 33
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, tail.x - x - 8)
                text: moreRow.name
                elide: Text.ElideMiddle
                font.pixelSize: KodosiTheme.fontFootnote
                font.weight: moreRow.selected ? Font.DemiBold : Font.Medium
            }
            Row {
                id: tail
                anchors.right: parent.right
                anchors.rightMargin: 7
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6

                PlainLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: moreRow.detail.length > 0
                    text: moreRow.detail
                    color: KodosiTheme.inkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                }
                Item {
                    id: rowSign
                    anchors.verticalCenter: parent.verticalCenter
                    width: childrenRect.width
                    height: 14
                }
            }
        }

        KFocusIndicator { active: moreRow.visualFocus }
    }

    component Chip: AbstractButton {
        id: chip

        property string name: ""
        property bool selected: false
        property bool shown: true
        property real fixedWidth: 26
        property real signWidth: 0
        property alias mark: markSlot.data
        property alias sign: signSlot.data
        property int glints: 0
        property int rings: 0
        property int glintDelay: 0
        property color ringColor: KodosiTheme.glowAmber

        y: 2
        implicitWidth: fixedWidth
        implicitHeight: 26
        hoverEnabled: true
        activeFocusOnTab: true
        Accessible.name: name
        Accessible.selected: selected
        scale: pressed ? 0.95 : 1

        Behavior on scale { NumberAnimation { duration: KodosiTheme.motionHover; easing.type: Easing.OutCubic } }

        background: Item {
            Bezel {
                anchors.fill: parent
                radius: KodosiTheme.radiusSm
                quiet: true
                pressed: chip.pressed
                fill: chip.selected ? KodosiTheme.accentSoft : KodosiTheme.mix(KodosiTheme.raised, KodosiTheme.ink, chip.hovered ? 0.05 : 0)
            }
            Cue {
                anchors.fill: parent
                radius: KodosiTheme.radiusSm
                glints: chip.glints
                rings: chip.rings
                delay: chip.glintDelay
                ringColor: chip.ringColor
            }
        }
        contentItem: Item {
            Item {
                id: markSlot
                x: 3
                width: 20
                height: 20
                anchors.verticalCenter: parent.verticalCenter
            }
            PlainLabel {
                id: chipLabel
                x: 29
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, 112)
                text: chip.name
                color: chip.shown || chip.selected ? KodosiTheme.ink : KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontCaption
                font.weight: chip.selected ? Font.DemiBold : Font.Medium
                elide: Text.ElideMiddle
            }
            Item {
                id: signSlot
                x: chipLabel.x + chipLabel.width + 6
                anchors.verticalCenter: parent.verticalCenter
                width: chip.signWidth
                height: 14
                visible: chip.signWidth > 0
            }
        }

        KFocusIndicator { active: chip.visualFocus }
    }

    component TerminalChip: Chip {
        id: terminal

        required property var modelData
        required property int index
        readonly property var session: modelData.value
        readonly property string readingItem: Models.Dosi.reading.item || ""
        readonly property bool unseen: Models.Sessions.attention.indexOf(session.id) >= 0
        readonly property string form: session.sign ? (session.asks || unseen ? session.sign : "") : unseen ? "changed" : ""
        readonly property bool waiting: ["hand", "question", "key"].indexOf(form) >= 0

        name: session.name || ""
        fixedWidth: modelData.width
        signWidth: form.length > 0 ? 14 : 0
        selected: Models.DesktopState.activeView === Models.DesktopState.Sessions && Models.DesktopState.selectedSessionId === session.id
        shown: Models.DesktopState.stagedSessionIds.indexOf(session.id) >= 0
        objectName: "dosi.marks.terminal." + (session.id || "")
        Accessible.id: objectName
        ToolTip.visible: hovered
        ToolTip.text: session.kind === "remote" && session.hostName ? session.name + " · " + session.hostName : session.name || ""
        ToolTip.delay: Tip.delay
        onClicked: Models.SessionActions.activate(session.id)
        onReadingItemChanged: {
            const reading = Models.Dosi.reading
            if (reading.kind === "terminals") {
                glintDelay = Math.min(index, 10) * 70
                glints += 1
            } else if (reading.kind === "terminal" && reading.sessionId === session.id) {
                glintDelay = 0
                glints += 1
            }
        }
        onWaitingChanged: {
            if (waiting)
                rings += 1
        }

        mark: AgentMark {
            size: 20
            program: terminal.session.program || ""
            session: terminal.session
            asleep: !terminal.shown
            opacity: terminalSpot.hidden ? 0 : 1

            FoldSpot {
                id: terminalSpot
                anchors.fill: parent
                key: "terminal:" + (terminal.session.id || "")
                place: "slot"
                session: terminal.session
                asleep: !terminal.shown
            }
        }
        sign: StatusSign {
            size: 9
            session: terminal.session
            unseen: terminal.unseen
        }
    }

    component RoomChip: Chip {
        id: roomChip

        required property var modelData
        required property int index
        readonly property var entry: modelData.value
        readonly property string readingItem: Models.Dosi.reading.item || ""
        readonly property bool unread: Models.Missions.unreadRooms.indexOf(entry.id) >= 0

        name: entry.name || ""
        fixedWidth: modelData.width
        signWidth: unread ? 8 : 0
        selected: Models.DesktopState.activeView === Models.DesktopState.Missions && Models.Missions.selectedMissionId === entry.id
        ringColor: KodosiTheme.accent
        objectName: "dosi.marks.room." + (entry.id || "")
        Accessible.id: objectName
        ToolTip.visible: hovered
        ToolTip.text: entry.name || ""
        ToolTip.delay: Tip.delay
        onClicked: {
            Models.DesktopState.activeView = Models.DesktopState.Missions
            Models.Missions.open(entry.id)
        }
        onReadingItemChanged: {
            const reading = Models.Dosi.reading
            if (reading.kind === "rooms") {
                glintDelay = Math.min(index, 10) * 70
                glints += 1
            } else if (reading.kind === "room" && reading.roomId === entry.id) {
                glintDelay = 0
                glints += 1
            }
        }
        onUnreadChanged: {
            if (unread)
                rings += 1
        }

        mark: RoomSigil {
            size: 20
            key: roomChip.entry.id || ""
            opacity: roomSpot.hidden ? 0 : 1

            FoldSpot {
                id: roomSpot
                anchors.fill: parent
                key: "room:" + (roomChip.entry.id || "")
                place: "slot"
                sigil: roomChip.entry.id || ""
            }
        }
        sign: NewDot {
            visible: roomChip.unread
        }
    }
}
