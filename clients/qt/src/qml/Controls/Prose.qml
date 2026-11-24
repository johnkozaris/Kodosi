pragma Singleton

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    function styled(html: string, look: Look): string {
        return html.replace(/<a href=/g, "<a style=\"color:" + look.accentStrong + ";text-decoration:none\" href=")
            .replace(/<code>/g, "<code style=\"font-family:monospace;background-color:rgba(" + Math.round(look.ink.r * 255) + ","
                + Math.round(look.ink.g * 255) + "," + Math.round(look.ink.b * 255) + ",0.08)\">")
    }
    function escaped(text: string): string {
        return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;")
    }
    function open(link: string): void {
        if (link.startsWith("https://"))
            Models.DesktopFiles.openWebUrl(link)
    }
}
