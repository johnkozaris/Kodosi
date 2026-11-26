package com.kodosi.mobile.canvas

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// The shared browser of a terminal's canvas: its video, and its tabs, pointers, menus and dialogs.
class BrowserFeed(pipes: VideoPipes, session: String, scope: CoroutineScope, decoders: List<JsonObject>) :
    VideoFeed(pipes, session, "browser", scope, decoders) {
    val window: StateFlow<JsonObject>
        field = MutableStateFlow(fields())
    val icons: StateFlow<Map<String, ImageBitmap>>
        field = MutableStateFlow(emptyMap<String, ImageBitmap>())

    // The last picture that a view of the browser showed, which the next view shows until the browser's
    // own pictures come. The browser's surface does not outlive its view, so the view copies it.
    val still: StateFlow<Bitmap?>
        field = MutableStateFlow<Bitmap?>(null)

    internal fun keep(picture: Bitmap) {
        still.value = picture
    }

    override fun freeStill() {
        still.value = null
    }

    override fun take(message: JsonObject) {
        window.value = message
        keepIcons(message)
    }

    private fun keepIcons(state: JsonObject) {
        val kept = icons.value.toMutableMap()
        (state["icons"] as? JsonObject)?.forEach { (tab, png) ->
            val bytes = runCatching { Base64.decode((png as? JsonPrimitive)?.contentOrNull.orEmpty(), Base64.DEFAULT) }
                .getOrNull()
            val icon = bytes?.takeIf { it.isNotEmpty() }?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
            if (icon == null) kept.remove(tab) else kept[tab] = icon.asImageBitmap()
        }
        val open = state.objects("tabs").mapNotNull { it.string("id") }.toSet()
        kept.keys.retainAll(open)
        if (kept != icons.value) icons.value = kept
    }
}
