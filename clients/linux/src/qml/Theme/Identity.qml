pragma Singleton

import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    readonly property var people: ["#3c8db5", "#8573d1", "#2f92a3", "#c2659a", "#b58428", "#b362b6", "#607fcc"]
    readonly property var rooms: ["#d98a5f", "#d9a441", "#c96f7e", "#8c7bd1", "#4f9bb0", "#b8743c", "#a86aae"]
    readonly property string selfName: Models.Account.displayName.length > 0 ? Models.Account.displayName : qsTr("You")

    function personTint(key: string): color {
        return people[Models.Appearance.stableIndex(key, people.length)]
    }

    function roomTint(key: string): color {
        return rooms[Models.Appearance.stableIndex(key, rooms.length)]
    }

    function initials(name: string): string {
        const letters = name.trim().split(/\s+/).slice(0, 2).map(part => String.fromCodePoint(part.codePointAt(0))).join("").toUpperCase()
        return letters.length > 0 ? letters : "·"
    }

    function firstName(name: string): string {
        return name.trim().split(/\s+/)[0] || name
    }

    function personName(userId: string): string {
        if (userId === Models.Account.userId)
            return selfName
        const name = Models.People.displayName(userId)
        return name.length > 0 ? name : qsTr("Someone")
    }

    function kind(program: string): string {
        const lower = program.toLowerCase()
        for (const name of ["claude", "codex", "copilot", "cursor"]) {
            if (lower.indexOf(name) >= 0)
                return name
        }
        return "shell"
    }

    function kindTint(kind: string): color {
        switch (kind) {
        case "claude": return "#d97757"
        case "codex": return "#5f6b7a"
        case "copilot": return "#8660d9"
        case "cursor": return "#4e7c8c"
        default: return "#54433a"
        }
    }

    function kindLabel(kind: string): string {
        switch (kind) {
        case "claude": return "Claude Code"
        case "codex": return "Codex"
        case "copilot": return "Copilot"
        case "cursor": return "Cursor"
        default: return qsTr("Shell")
        }
    }

    function count(value: int, one: string, many: string): string {
        return value === 1 ? one : many.arg(value)
    }
}
