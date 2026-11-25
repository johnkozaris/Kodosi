package com.kodosi.mobile.canvas

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

@Serializable
data class Canvas(
    val items: List<CanvasItem> = emptyList(),
    val browser: CanvasBrowser? = null,
    // The screen of the terminal's computer, which only its owner's devices see.
    val screen: Boolean = false,
    val open: CanvasOpen? = null,
    val change: CanvasOpen? = null
) {
    val latestNotice: CanvasOpen? get() = listOfNotNull(open, change).maxByOrNull { it.at }

    // The screen does not change as a file or the browser does: it shows itself when an agent or a
    // person shows it.
    fun partChanges(): Map<String, Long> = buildMap {
        items.maxOfOrNull { it.changed }?.let { put("files", it) }
        browser?.let { put("browser", it.changed) }
        if (screen) put("screen", 0L)
    }

    fun shownFile(chosen: String?): CanvasItem? =
        items.firstOrNull { it.key == chosen } ?: items.maxByOrNull { it.changed }

    fun openKey(at: String?, closed: Set<String>): String {
        if (at == "browser" || at == "screen" || items.any { it.key == at }) return at!!
        val parts = partChanges()
        val showing = parts.filterKeys { it !in closed }
        val part = (showing.ifEmpty { parts }).maxByOrNull { it.value }?.key ?: "browser"
        return if (part == "files") items.maxBy { it.changed }.key else part
    }
}

// The part of the canvas that a key of it shows.
fun canvasPart(key: String): String = if (key == "browser" || key == "screen") key else "files"

@Serializable
data class CanvasOpen(val key: String, val at: Long)

@Serializable
data class CanvasItem(
    val key: String,
    val name: String,
    val kind: String,
    val view: String? = null,
    val revision: Long,
    val bytes: Long,
    val changed: Long,
    val missing: Boolean = false
)

@Serializable
data class CanvasBrowser(
    val state: String,
    val changed: Long,
    val tabs: List<BrowserTab> = emptyList(),
    val getting: Int? = null,
    val doing: JsonObject? = null,
    val problem: String? = null,
    val ownSignIns: Boolean = false
)

@Serializable
data class BrowserTab(val id: String, val title: String, val url: String, val active: Boolean = false)

@Serializable
data class BrowserPage(val title: String, val address: String)
