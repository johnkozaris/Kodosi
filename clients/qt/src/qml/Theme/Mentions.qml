pragma Singleton

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    function targets(members: var, sessionIds: var): var {
        const list = []
        for (const member of members) {
            const name = member.displayName || member.handle
            if (name)
                list.push({ id: member.userId, name: name, detail: "@" + member.handle, kind: "person", program: "" })
        }
        for (const id of sessionIds) {
            const session = Models.Sessions.presentationForSession(id)
            if (session.name)
                list.push({ id: id, name: session.name, detail: session.hostLabel || "", kind: "terminal", program: session.program || "" })
        }
        return list
    }

    function escaped(text: string): string {
        return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;")
    }

    function wordCharacter(character: string): bool {
        return character.toLowerCase() !== character.toUpperCase() || /[0-9_]/.test(character)
    }

    function segments(text: string, targets: var): var {
        const ordered = targets.slice().sort((left, right) => right.name.length - left.name.length)
        const result = []
        let plain = ""
        let index = 0
        while (index < text.length) {
            let matched = null
            if (text[index] === "@" && (index === 0 || !wordCharacter(text[index - 1]))) {
                for (const target of ordered) {
                    const end = index + 1 + target.name.length
                    if (text.substring(index + 1, end).toLowerCase() === target.name.toLowerCase()
                            && (end >= text.length || !wordCharacter(text[end]))) {
                        matched = target
                        break
                    }
                }
            }
            if (matched) {
                if (plain.length > 0)
                    result.push({ text: plain, target: null })
                plain = ""
                result.push({ text: text.substring(index, index + 1 + matched.name.length), target: matched })
                index += 1 + matched.name.length
            } else {
                plain += text[index]
                index += 1
            }
        }
        if (plain.length > 0)
            result.push({ text: plain, target: null })
        return result
    }

    function mentions(parts: var, userId: string): bool {
        return parts.some(segment => segment.target && segment.target.kind === "person" && segment.target.id === userId)
    }

    function tint(target: var, selfId: string): color {
        if (target.kind === "person")
            return target.id === selfId ? KodosiTheme.accentStrong : KodosiTheme.mix(Identity.personTint(target.id), KodosiTheme.ink, 0.25)
        const kind = Models.DesktopSettings.agentOf(target.program || "")
        return kind === "shell" ? KodosiTheme.accentStrong : KodosiTheme.mix(Identity.kindTint(kind), KodosiTheme.ink, 0.2)
    }

    function html(parts: var, selfId: string): string {
        return "<span style=\"white-space:pre-wrap\">" + parts.map(segment => {
            const body = escaped(segment.text)
            if (!segment.target)
                return body
            const style = "color:" + tint(segment.target, selfId) + ";font-weight:600;text-decoration:none"
            const word = body.replace(/ /g, "&nbsp;")
            return segment.target.kind === "terminal"
                ? "<a href=\"kodosi-mention://terminal/" + escaped(segment.target.id) + "\" style=\"" + style + "\">" + word + "</a>"
                : "<span style=\"" + style + "\">" + word + "</span>"
        }).join("") + "</span>"
    }

    function chips(parts: var, selfId: string): var {
        const result = []
        let position = 0
        for (const segment of parts) {
            if (segment.target)
                result.push({ start: position, end: position + segment.text.length, color: tint(segment.target, selfId) })
            position += segment.text.length
        }
        return result
    }

    function query(text: string): string {
        const at = text.lastIndexOf("@")
        if (at < 0 || (at > 0 && wordCharacter(text[at - 1])))
            return ""
        const fragment = text.substring(at + 1)
        return fragment.length > 40 || fragment.indexOf("\n") >= 0 ? "" : "@" + fragment
    }

    function suggestions(query: string, targets: var, selfId: string): var {
        if (query.length === 0)
            return []
        const needle = query.substring(1).toLowerCase()
        return targets.filter(target => {
            if (target.id === selfId)
                return false
            const name = target.name.toLowerCase()
            return name.startsWith(needle) || name.split(" ").some(word => needle.length > 0 && word.startsWith(needle))
        }).slice(0, 6)
    }

    function complete(text: string, target: var): string {
        const at = text.lastIndexOf("@")
        return (at < 0 ? text : text.substring(0, at)) + "@" + target.name + " "
    }
}
