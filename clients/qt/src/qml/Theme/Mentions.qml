pragma Singleton

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    readonly property string terminalLink: "kodosi-mention://terminal/"

    function targets(members: var, sessionIds: var): var {
        const list = []
        for (const member of members) {
            const name = member.displayName || member.handle
            if (name)
                list.push({ id: member.userId, name: name, detail: "@" + member.handle, kind: "person", program: "", handle: member.handle || "" })
        }
        for (const id of sessionIds) {
            const session = Models.Sessions.presentationForSession(id)
            if (session.name)
                list.push({ id: id, name: session.name, detail: session.hostLabel || "", kind: "terminal", program: session.program || "" })
        }
        return list
    }

    function peerTargets(peers: var): var {
        return peers.map(peer => ({ id: peer.terminal, name: peer.name, detail: peer.place, kind: "terminal", program: peer.agent }))
    }

    // A chip opens the terminal of a named agent only while the chat reaches that agent.
    function mentionTargets(mentions: var, peers: var): var {
        return mentions.map(mention => ({ id: (peers.find(peer => peer.name === mention.name) || { terminal: "" }).terminal,
            name: mention.name, kind: "terminal", program: mention.agent }))
    }

    function wordCharacter(character: string): bool {
        return character.toLowerCase() !== character.toUpperCase() || /[0-9_]/.test(character)
    }

    function segments(text: string, targets: var): var {
        const ordered = []
        for (const target of targets) {
            for (const word of [target.name, target.handle || ""]) {
                if (word)
                    ordered.push({ target: target, word: word })
            }
        }
        ordered.sort((left, right) => right.word.length - left.word.length)
        const result = []
        let plain = ""
        let index = 0
        while (index < text.length) {
            let matched = null
            if (text[index] === "@" && (index === 0 || !wordCharacter(text[index - 1]))) {
                for (const candidate of ordered) {
                    const end = index + 1 + candidate.word.length
                    if (text.substring(index + 1, end).toLowerCase() === candidate.word.toLowerCase()
                            && (end >= text.length || !wordCharacter(text[end]))) {
                        matched = candidate
                        break
                    }
                }
            }
            if (matched) {
                if (plain.length > 0)
                    result.push({ text: plain, target: null })
                plain = ""
                result.push({ text: text.substring(index, index + 1 + matched.word.length), target: matched.target })
                index += 1 + matched.word.length
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

    function tint(target: var, selfId: string, look: Look): color {
        if (target.kind === "person")
            return target.id === selfId ? look.accentStrong : KodosiTheme.mix(Identity.personTint(target.id), look.ink, 0.25)
        const kind = Models.DesktopSettings.agentOf(target.program || "")
        return kind === "shell" ? look.accentStrong : KodosiTheme.mix(Identity.kindTint(kind), look.ink, 0.2)
    }

    // `marked` words between the mentions are Markdown, as a terminal's chat shows a person's words.
    function html(parts: var, selfId: string, look: Look, marked: bool): string {
        return "<span style=\"white-space:pre-wrap\">" + parts.map(segment => {
            if (!segment.target)
                return marked ? Models.Markdown.text(segment.text) : Qt.escapeHtml(segment.text)
            const style = "color:" + tint(segment.target, selfId, look) + ";font-weight:600;text-decoration:none"
            const word = Qt.escapeHtml(segment.text).replace(/ /g, "&nbsp;")
            return segment.target.kind === "terminal" && segment.target.id
                ? "<a style=\"" + style + "\" href=\"" + terminalLink + Qt.escapeHtml(segment.target.id) + "\">" + word + "</a>"
                : "<span style=\"" + style + "\">" + word + "</span>"
        }).join("") + "</span>"
    }

    function terminal(link: string): string {
        return link.startsWith(terminalLink) ? link.substring(terminalLink.length) : ""
    }

    // Finds each mention in `shown`, the text as its field shows it, where Markdown can have
    // removed characters before the mention.
    function chips(parts: var, selfId: string, look: Look, shown: string): var {
        const result = []
        let from = 0
        for (const segment of parts) {
            if (!segment.target)
                continue
            const word = segment.text.replace(/ /g, "\u00a0")
            let start = shown.indexOf(word, from)
            while (start >= 0 && ((start > 0 && wordCharacter(shown[start - 1]))
                    || (start + word.length < shown.length && wordCharacter(shown[start + word.length]))))
                start = shown.indexOf(word, start + 1)
            if (start < 0)
                break
            result.push({ start: start, end: start + word.length, color: tint(segment.target, selfId, look) })
            from = start + word.length
        }
        return result
    }

    function query(text: string): string {
        const at = text.lastIndexOf("@")
        if (at < 0 || (at > 0 && wordCharacter(text[at - 1])))
            return ""
        const fragment = text.substring(at + 1)
        return fragment.length > 40 || fragment.indexOf("\n") >= 0 || fragment.split(" ").length > 3 ? "" : "@" + fragment
    }

    function suggestions(query: string, targets: var, selfId: string): var {
        if (query.length === 0)
            return []
        const needle = query.substring(1).toLowerCase()
        const candidates = targets.filter(target => target.id !== selfId)
        const starting = candidates.filter(target => target.name.toLowerCase().startsWith(needle))
        return starting.concat(candidates.filter(target => !target.name.toLowerCase().startsWith(needle)
            && target.name.toLowerCase().includes(needle))).slice(0, 6)
    }

    // The text with an `@` at its end for a new mention, unless it ends with the start of one.
    function start(text: string): string {
        if (query(text).length > 0)
            return text
        return text + (text.length === 0 || text.endsWith(" ") || text.endsWith("\n") ? "@" : " @")
    }

    function complete(text: string, target: var): string {
        const at = text.lastIndexOf("@")
        return (at < 0 ? text : text.substring(0, at)) + "@" + target.name + " "
    }
}
