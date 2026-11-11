package com.kodosi.mobile.canvas

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

@Serializable
data class Canvas(
    val sequence: Long,
    val items: List<CanvasItem> = emptyList(),
    val browser: CanvasBrowser? = null,
    val open: CanvasOpen? = null,
    val change: CanvasOpen? = null
) {
    val latestNotice: CanvasOpen? get() = listOfNotNull(open, change).maxByOrNull { it.at }

    fun partChanges(): Map<String, Long> = buildMap {
        items.maxOfOrNull { it.changed }?.let { put("files", it) }
        browser?.let { put("browser", it.changed) }
    }

    fun openKey(at: String?, closed: Set<String>): String {
        if (at == "browser" || items.any { it.key == at }) return at!!
        val newest = items.maxByOrNull { it.changed }
        val parts = partChanges()
        val showing = parts.filterKeys { it !in closed }
        return if ((showing.ifEmpty { parts }).maxByOrNull { it.value }?.key == "files") newest!!.key else "browser"
    }
}

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
    val width: Int,
    val height: Int,
    val getting: Int? = null,
    val doing: JsonObject? = null,
    val last: JsonObject? = null,
    val problem: String? = null
)

@Serializable
data class BrowserTab(val id: String, val title: String, val url: String, val active: Boolean = false)

@Serializable
data class BrowserPage(val title: String, val address: String)
