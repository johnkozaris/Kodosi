package com.kodosi.mobile

import android.content.Context
import androidx.core.content.edit
import com.kodosi.mobile.canvas.BrowserPage
import com.kodosi.mobile.canvas.Canvas
import com.kodosi.mobile.runtime.RuntimeJson
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.encodeToString

data class Preferences(
    val appearance: String = "system",
    val textSize: Int = 0,
    val keepScreenOn: Boolean = false,
    val lock: Boolean = false,
    val disabledViewers: Set<String> = emptySet(),
    val documentPages: Boolean = true,
    val documentDark: Boolean = true,
    val closedCanvas: Set<String> = emptySet(),
    val notifications: Boolean = true
) {
    fun closedCanvasParts(session: String, canvas: Canvas?): Set<String> =
        canvas?.partChanges().orEmpty().filter { (part, changed) -> "$session/$part/$changed" in closedCanvas }.keys
}

class AppPreferences(context: Context) {
    private val saved = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)
    private val mutableState = MutableStateFlow(
        Preferences(
            saved.getString("appearance", "system").orEmpty(),
            saved.getInt("textSize", 0),
            saved.getBoolean("keepScreenOn", false),
            saved.getBoolean("lock", false),
            saved.getStringSet("disabledViewers", emptySet()).orEmpty().toSet(),
            saved.getBoolean("documentPages", true),
            saved.getBoolean("documentDark", true),
            saved.getStringSet("canvas/closed", emptySet()).orEmpty().toSet(),
            saved.getBoolean("notifications", true)
        )
    )
    val state: StateFlow<Preferences> = mutableState
    private val mutableHistory = MutableStateFlow(
        runCatching {
            RuntimeJson.decodeFromString<List<BrowserPage>>(saved.getString("browser/history", "[]").orEmpty())
        }
            .getOrDefault(emptyList()).take(100)
    )
    val browserHistory: StateFlow<List<BrowserPage>> = mutableHistory
    val notificationPermissionAsked: Boolean get() = saved.getBoolean("notifications/asked", false)

    fun notificationPermissionAsked() {
        saved.edit { putBoolean("notifications/asked", true) }
    }

    val launchComputer: String? get() = saved.getString("launch/computer", null)
    fun launchFolder(computer: String): String? = saved.getString("launch/folder/$computer", null)

    fun launched(computer: String, folder: String?) {
        saved.edit {
            putString("launch/computer", computer)
            putString("launch/folder/$computer", folder)
        }
    }

    fun account(user: String?) {
        if (saved.getString("launch/account", null) == user) return
        saved.edit {
            saved.all.keys.filter { it.startsWith("launch/") }.forEach(::remove)
            putString("launch/account", user)
            remove("browser/history")
            remove("canvas/closed")
        }
        mutableHistory.value = emptyList()
        mutableState.value = state.value.copy(closedCanvas = emptySet())
    }

    fun canvasPart(session: String, part: String, open: Boolean, changed: Long = 0) {
        val prefix = "$session/$part/"
        update {
            val remaining = it.closedCanvas.filterNot { key -> key.startsWith(prefix) }.toSet()
            val closed = if (open) remaining else remaining + "$prefix$changed"
            it.copy(closedCanvas = closed.toList().takeLast(512).toSet())
        }
    }

    fun browsed(title: String, address: String) {
        if (address.isBlank() || address == "about:blank" || address.length > 8192) return
        val page = BrowserPage(title.take(256).ifBlank { address }, address)
        if (mutableHistory.value.firstOrNull() == page) return
        val next = (listOf(page) + mutableHistory.value.filter { it.address != address }).take(100)
        saved.edit { putString("browser/history", RuntimeJson.encodeToString(next)) }
        mutableHistory.value = next
    }

    fun update(change: (Preferences) -> Preferences) {
        val next = change(state.value)
        if (next == state.value) return
        saved.edit {
            putString("appearance", next.appearance)
            putInt("textSize", next.textSize)
            putBoolean("keepScreenOn", next.keepScreenOn)
            putBoolean("lock", next.lock)
            putStringSet("disabledViewers", next.disabledViewers)
            putBoolean("documentPages", next.documentPages)
            putBoolean("documentDark", next.documentDark)
            putStringSet("canvas/closed", next.closedCanvas)
            putBoolean("notifications", next.notifications)
        }
        mutableState.value = next
    }

    companion object {
        const val FILE = "kodosi-preferences"
    }
}
