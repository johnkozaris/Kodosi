pragma Singleton

import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    readonly property var people: ["#3c8db5", "#8573d1", "#2f92a3", "#c2659a", "#b58428", "#b362b6", "#607fcc"]
    readonly property var projects: ["#d98a5f", "#d9a441", "#c96f7e", "#8c7bd1", "#4f9bb0", "#b8743c", "#a86aae"]
    readonly property string selfName: Models.Account.displayName.length > 0 ? Models.Account.displayName : qsTr("You")

    function personTint(key: string): color {
        return people[Models.Appearance.stableIndex(key, people.length)]
    }

    function projectTint(key: string): color {
        return projects[Models.Appearance.stableIndex(key, projects.length)]
    }

    function initials(name: string): string {
        const letters = name.trim().split(/\s+/).filter(part => part.length > 0).slice(0, 2)
            .map(part => String.fromCodePoint(part.codePointAt(0))).join("").toUpperCase()
        return letters.length > 0 ? letters : "·"
    }

    function firstName(name: string): string {
        return name.trim().split(/\s+/)[0] || name
    }

    function personName(userId: string): string {
        if (userId === Models.Account.userId)
            return selfName
        const name = Models.People.displayName(userId)
        if (name.length > 0)
            return name
        const member = Models.Projects.members.find(member => member.userId === userId)
        return member ? (member.displayName || member.handle) : qsTr("Someone")
    }

    function accountTint(kind: string, account: int): color {
        const shades = {
            "claude": ["#d97757", "#c4301c", "#d38a0f", "#6b3420", "#c9456b"],
            "codex": ["#5f6b7a", "#2e5a8c", "#2f7d74", "#3a3f4a", "#6e5a7e"],
            "copilot": ["#8660d9", "#4b3bb0", "#b44bc8", "#5a2f6e", "#5b7be0"]
        }[kind]
        return shades && account > 0 ? shades[account % shades.length] : kindTint(kind)
    }

    function kindTint(kind: string): color {
        switch (kind) {
        case "claude": return "#d97757"
        case "codex": return "#5f6b7a"
        case "copilot": return "#8660d9"
        case "cursor": return "#4e7c8c"
        case "gemini": return "#5579c9"
        case "opencode": return "#6f6258"
        default: return "#54433a"
        }
    }
}
