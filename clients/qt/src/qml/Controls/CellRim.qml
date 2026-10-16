pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import "Typing.js" as Typing

Item {
    id: root

    property string mode: "working"
    property real fraction: 0
    property real radius: 8
    property real lineWidth: 2
    property real seconds: Date.now() / 1000

    readonly property bool still: KodosiTheme.reduceMotion
    readonly property real trackWidth: Math.max(0, width - lineWidth)
    readonly property real trackHeight: Math.max(0, height - lineWidth)
    readonly property real corner: Math.min(Math.max(radius - lineWidth / 2, 0), Math.min(trackWidth, trackHeight) / 2)
    readonly property real length: Math.max(0, 2 * (trackWidth - 2 * corner) + 2 * (trackHeight - 2 * corner) + 2 * Math.PI * corner)
    readonly property int count: Math.max(8, Math.floor(length / (lineWidth * 2.5)))
    readonly property real cellLength: length / count * 0.6
    readonly property int head: still ? 0 : Typing.step(seconds)
    readonly property bool blinkLit: still || Math.floor(seconds / 0.53) % 2 === 0
    readonly property var cooling: [KodosiTheme.accentStrong, KodosiTheme.accent,
        KodosiTheme.mix(KodosiTheme.accent, KodosiTheme.accentSoft, 0.45), KodosiTheme.mix(KodosiTheme.accent, KodosiTheme.accentSoft, 0.72)]

    function coolingColor(age) {
        return age < 0.14 ? cooling[0] : age < 0.45 ? cooling[1] : age < 0.9 ? cooling[2] : age < 1.4 ? cooling[3] : KodosiTheme.accentSoft
    }
    function cellColor(index) {
        if (mode === "progress") {
            const reach = fraction * count - index
            return reach >= 1 ? KodosiTheme.accent : reach <= 0 ? KodosiTheme.accentSoft
                : KodosiTheme.mix(KodosiTheme.accent, KodosiTheme.accentSoft, 1 - reach)
        }
        const ago = Typing.wrapped(head - index, count)
        return still ? coolingColor(ago < 4 ? ago * 0.25 : 9) : coolingColor(seconds - Typing.time(head - ago))
    }
    // The point at `at` of the way round, clockwise from the middle of the top
    // edge, and the direction of travel there.
    function place(at) {
        const left = lineWidth / 2
        const top = lineWidth / 2
        const right = left + trackWidth
        const bottom = top + trackHeight
        const r = corner
        const pieces = [
            { line: [left + trackWidth / 2, top, right - r, top] },
            { arc: [right - r, top + r, -Math.PI / 2] },
            { line: [right, top + r, right, bottom - r] },
            { arc: [right - r, bottom - r, 0] },
            { line: [right - r, bottom, left + r, bottom] },
            { arc: [left + r, bottom - r, Math.PI / 2] },
            { line: [left, bottom - r, left, top + r] },
            { arc: [left + r, top + r, Math.PI] },
            { line: [left + r, top, left + trackWidth / 2, top] }
        ]
        let rest = (at - Math.floor(at)) * length
        for (const piece of pieces) {
            if (piece.line) {
                const [x1, y1, x2, y2] = piece.line
                const span = Math.hypot(x2 - x1, y2 - y1)
                if (rest <= span && span > 0)
                    return { x: x1 + (x2 - x1) * rest / span, y: y1 + (y2 - y1) * rest / span, angle: Math.atan2(y2 - y1, x2 - x1) }
                rest -= span
            } else {
                const [cx, cy, start] = piece.arc
                const span = Math.PI / 2 * r
                if (rest <= span && span > 0) {
                    const angle = start + rest / r
                    return { x: cx + Math.cos(angle) * r, y: cy + Math.sin(angle) * r, angle: angle + Math.PI / 2 }
                }
                rest -= span
            }
        }
        return { x: left + trackWidth / 2, y: top, angle: 0 }
    }

    Accessible.ignored: true

    FrameAnimation {
        property real gathered: 0

        running: root.visible && !root.still && KodosiTheme.shown
        onTriggered: {
            gathered += frameTime
            if (gathered >= (root.mode === "working" ? 1 / 30 : 0.1)) {
                root.seconds += gathered
                gathered = 0
            }
        }
    }
    Repeater {
        model: root.count

        Rectangle {
            required property int index
            readonly property var spot: root.place((index + 0.5) / root.count)
            readonly property real across: Math.abs(Math.cos(spot.angle))
            readonly property real along: Math.abs(Math.sin(spot.angle))
            readonly property bool turning: Math.abs(across - along) < 0.4
            readonly property real side: (root.cellLength + root.lineWidth) / 2

            width: turning ? side : across >= along ? root.cellLength : root.lineWidth
            height: turning ? side : across >= along ? root.lineWidth : root.cellLength
            x: Math.round(spot.x - width / 2)
            y: Math.round(spot.y - height / 2)
            radius: root.lineWidth * 0.3
            color: root.cellColor(index)
        }
    }
    Rectangle {
        readonly property int index: root.mode === "working" ? Typing.wrapped(root.head, root.count)
            : Math.min(root.count - 1, Math.floor(root.fraction * root.count))
        readonly property var spot: root.place((index + 0.5) / root.count)
        readonly property real idle: root.seconds - Typing.time(root.head)

        visible: root.mode === "working" ? root.still || idle < 0.24 || Math.floor((idle - 0.24) / 0.42) % 2 === 1
            : root.fraction < 1 && root.blinkLit
        width: Math.max(root.lineWidth * 1.5, root.cellLength)
        height: root.lineWidth * 2.6
        x: Math.round(spot.x - width / 2)
        y: Math.round(spot.y - height / 2)
        radius: root.lineWidth * 0.35
        color: KodosiTheme.accentStrong
    }
}
