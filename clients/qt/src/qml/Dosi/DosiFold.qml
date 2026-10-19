pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    readonly property string place: !Models.DesktopState.sidebarOpen ? "rail" : Models.DesktopState.dosiOpen ? "slot" : "row"
    property string previous: place
    property var planes: ({})

    function collect(where) {
        const found = {}
        for (const spot of Fold.spots) {
            if (spot.place !== where || spot.width <= 0)
                continue
            let rect = spot.mapToItem(root, 0, 0, spot.width, spot.height)
            let shown = true
            if (spot.tucked) {
                rect = Qt.rect(rect.x + rect.width * 0.3, rect.y + rect.height * 0.3, rect.width * 0.4, rect.height * 0.4)
                shown = false
            } else if (spot.view) {
                const view = spot.view.mapToItem(root, 0, 0, spot.view.width, spot.view.height)
                const middle = rect.y + rect.height / 2
                if (middle < view.y || middle > view.y + view.height) {
                    rect = Qt.rect(rect.x, Math.min(Math.max(rect.y, view.y), view.y + view.height - rect.height), rect.width, rect.height)
                    shown = false
                }
            }
            if (rect.y + rect.height <= 0 || rect.y >= height)
                continue
            found[spot.key] = { rect: rect, shown: shown, asleep: spot.asleep, session: spot.session, room: spot.sigil, fill: spot.fill, radius: spot.radius }
        }
        return found
    }
    function clear() {
        for (const key of Object.keys(planes)) {
            const plane = planes[key]
            Fold.land(key + "@" + plane.where)
            plane.destroy()
        }
        planes = {}
    }
    function fly() {
        const arrivals = collect(place)
        const keys = Object.keys(planes)
        const marks = keys.filter(key => arrivals[key] !== undefined && (key.startsWith("terminal:") || key.startsWith("room:")))
        marks.sort((a, b) => arrivals[a].rect.y - arrivals[b].rect.y || arrivals[a].rect.x - arrivals[b].rect.x)
        if (place === "row")
            marks.reverse()
        for (const key of keys) {
            const plane = planes[key]
            if (arrivals[key] === undefined) {
                plane.vanish()
                continue
            }
            plane.go(arrivals[key], Math.min(6, Math.max(0, marks.indexOf(key))))
        }
    }

    Accessible.ignored: true
    onPlaceChanged: {
        clear()
        const departures = collect(previous)
        previous = place
        if (KodosiTheme.reduceMotion || !KodosiTheme.shown)
            return
        const keys = Object.keys(departures)
        Fold.launch(keys.map(key => key + "@" + place))
        const next = {}
        for (const key of keys)
            next[key] = flight.createObject(root, { key: key, where: place, from: departures[key] })
        planes = next
        arrival.restart()
    }

    Timer {
        id: arrival
        interval: 24
        onTriggered: root.fly()
    }
    Component {
        id: flight

        Item {
            id: plane

            required property string key
            required property string where
            required property var from
            property var to: from
            property int delay: 0
            property color fill: from.fill
            property real corner: from.radius
            property real fade: from.shown ? 1 : 0

            function go(target, order) {
                to = target
                delay = order * 35
                motion.start()
            }
            function vanish() {
                ending.start()
            }
            function finish() {
                Fold.land(plane.key + "@" + plane.where)
                delete root.planes[plane.key]
                plane.destroy()
            }

            x: from.rect.x
            y: from.rect.y
            width: from.rect.width
            height: from.rect.height
            z: key === "box" ? 0 : 1
            opacity: fade

            Loader {
                anchors.fill: parent
                sourceComponent: plane.key.startsWith("terminal:") ? terminal
                    : plane.key.startsWith("room:") ? room
                    : plane.key === "face" ? face : shape
            }
            Component {
                id: terminal
                AgentMark {
                    size: plane.width
                    program: plane.from.session ? plane.from.session.program || "" : ""
                    session: plane.from.session
                    asleep: plane.to.asleep
                }
            }
            Component {
                id: room
                RoomSigil {
                    size: plane.width
                    key: plane.from.room
                }
            }
            Component {
                id: face
                DosiFace {
                    size: plane.width
                    mood: Models.Dosi.mood
                    still: true
                }
            }
            Component {
                id: shape
                Item {
                    Raised {
                        anchors.fill: parent
                        visible: plane.key === "box"
                        radius: plane.corner
                        fill: plane.fill
                    }
                    Rectangle {
                        anchors.fill: parent
                        visible: plane.key !== "box"
                        radius: plane.corner
                        color: plane.fill

                        KIcon {
                            anchors.centerIn: parent
                            width: 13
                            height: 13
                            visible: plane.key === "new"
                            name: "plus"
                            color: KodosiTheme.accentInk
                            strokeWidth: 2.6
                        }
                    }
                }
            }
            SequentialAnimation {
                id: motion
                PauseAnimation { duration: plane.delay }
                ParallelAnimation {
                    NumberAnimation { target: plane; property: "x"; to: plane.to.rect.x; duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic }
                    NumberAnimation { target: plane; property: "y"; to: plane.to.rect.y; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
                    NumberAnimation { target: plane; property: "width"; to: plane.to.rect.width; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
                    NumberAnimation { target: plane; property: "height"; to: plane.to.rect.height; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
                    NumberAnimation { target: plane; property: "fade"; to: plane.to.shown ? 1 : 0; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
                    NumberAnimation { target: plane; property: "corner"; to: plane.to.radius; duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic }
                    ColorAnimation { target: plane; property: "fill"; to: plane.to.fill; duration: KodosiTheme.motionSpring }
                }
                ScriptAction { script: plane.finish() }
            }
            SequentialAnimation {
                id: ending
                NumberAnimation { target: plane; property: "fade"; to: 0; duration: KodosiTheme.motionFade; easing.type: Easing.OutCubic }
                ScriptAction { script: plane.finish() }
            }
        }
    }
}
