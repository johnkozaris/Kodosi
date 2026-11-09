package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeFailure
import com.kodosi.mobile.runtime.RuntimeHandle
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.TerminalLink
import com.kodosi.mobile.runtime.UuidV7
import java.nio.ByteBuffer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull

/** The size of a terminal view, in cells and in pixels. */
data class Viewport(val cols: Int, val rows: Int, val cellWidth: Int, val cellHeight: Int)

/** What a terminal view needs to say to the person. */
interface TerminalWords {
    val fellBehind: String
    val couldNotConnect: String
    val invalidControl: String
}

/**
 * One live view of a terminal on another computer: it opens the terminal, follows its checkpoint
 * and data into Ghostty's state, takes the focus and the size while it shows, and sends input.
 */
class TerminalConnection(
    val sessionId: String,
    private val runtime: RuntimeHandle,
    private val commands: CommandSink,
    private val store: TerminalStore,
    private val scope: CoroutineScope,
    private val words: TerminalWords
) {
    sealed interface State {
        data object Connecting : State

        data object Rendered : State

        data class Closed(val reason: String?) : State

        data class Failed(val message: String) : State
    }

    val kernel = TerminalKernel()
    private val mutableState = MutableStateFlow<State>(State.Connecting)
    val state: StateFlow<State> = mutableState

    /** Called on any thread when the screen changes. */
    @Volatile
    var onChange: () -> Unit = {}

    @Volatile
    private var link: Link? = null

    @Volatile
    private var incarnation = ""
    private var opening: Job? = null
    private var restored = false
    private var focus: String? = null
    private var focused = false
    private var focusRetried = false
    private var viewport: Viewport? = null
    private var sizedTo: Viewport? = null
    private var resizing: Job? = null
    private var surfaceGeneration = 0L
    private val input = Channel<ByteArray>(INPUT_CAPACITY)
    private var shown = false

    init {
        scope.launch(Dispatchers.Default) {
            for (bytes in input) deliver(bytes)
        }
    }

    /** The screen shows this terminal or stops showing it. */
    fun show(visible: Boolean) {
        if (shown == visible) return
        shown = visible
        store.watch(sessionId, visible)
        if (visible) {
            open()
            refresh()
        } else {
            blur()
        }
    }

    /** Follows the catalog: connects when the terminal is reachable, again after it restarted. */
    fun refresh() {
        val session = store.session(sessionId) ?: return
        val current = link
        if (current != null && session.incarnationId != incarnation) {
            leave(current)
        }
        if (link == null && shown && session.isConnected) connect(session)
        if (shown && session.canControl && restored && focus == null && !focused) requestFocus()
    }

    fun setViewport(next: Viewport) {
        if (viewport == next) return
        viewport = next
        surfaceGeneration++
        scheduleResize()
    }

    /** Sends the size of this view again, so that the terminal takes it. */
    fun takeSize() {
        sizedTo = null
        scheduleResize()
    }

    fun send(bytes: ByteArray) {
        if (bytes.isEmpty() || state.value != State.Rendered) return
        kernel.scroll(0)
        input.trySend(bytes)
    }

    /** Opens the terminal again and connects to it once it is reachable. */
    fun retry() {
        link?.let(::leave)
        mutableState.value = State.Connecting
        open()
        refresh()
    }

    /** Ends the view: it leaves the terminal and frees Ghostty's state. */
    fun close() {
        show(false)
        link?.let(::leave)
        store.disconnect(sessionId)
        input.close()
        kernel.close()
    }

    private fun open() {
        if (opening != null) return
        val session = store.session(sessionId)
        if (session == null || !session.canOpen) return
        opening = scope.launch {
            try {
                store.open(sessionId)
            } catch (failure: RuntimeFailure) {
                mutableState.value = State.Failed(failure.message.orEmpty())
            }
            opening = null
        }
    }

    private fun connect(session: RuntimeSession) {
        val next = Link(UuidV7.generate(), TerminalLink.nextGeneration())
        link = next
        incarnation = session.incarnationId
        restored = false
        focused = false
        focus = null
        sizedTo = null
        mutableState.value = State.Connecting
        scope.launch {
            for (wait in CONNECT_BACKOFF_MS + listOf(0L)) {
                if (link !== next) return@launch
                val result = runtime.connect(sessionId, next)
                when {
                    result == Ffi.OK -> return@launch

                    result == Ffi.BUSY && wait > 0 -> delay(wait)

                    else -> {
                        fail(next, words.couldNotConnect)
                        return@launch
                    }
                }
            }
        }
    }

    private fun leave(current: Link) {
        blur()
        current.active = false
        runtime.disconnect(sessionId, current)
        if (link === current) link = null
        restored = false
        resizing?.cancel()
    }

    private fun fail(failed: Link, message: String) {
        if (link !== failed) return
        leave(failed)
        mutableState.value = State.Failed(message)
    }

    private fun changed() = onChange()

    private fun requestFocus() {
        val current = link ?: return
        val request = UuidV7.generate()
        focus = request
        val result = commands.send("session.focus", identity(current) + mapOf("requestId" to JsonPrimitive(request)))
        if (result != Ffi.OK) focus = null
    }

    private fun blur() {
        val current = link ?: return
        if (!focused && focus == null) return
        focused = false
        focus = null
        focusRetried = false
        commands.send("session.blur", identity(current))
    }

    private fun identity(current: Link): Map<String, JsonElement> = mapOf(
        "sessionId" to JsonPrimitive(sessionId),
        "clientId" to JsonPrimitive(current.subscriptionId),
        "subscriptionGeneration" to JsonPrimitive(current.generation),
        "expectedRuntimeIncarnationId" to JsonPrimitive(incarnation)
    )

    private fun scheduleResize() {
        resizing?.cancel()
        resizing = scope.launch {
            delay(RESIZE_SETTLE_MS)
            resize()
        }
    }

    private fun resize() {
        val current = link ?: return
        val size = viewport ?: return
        if (!focused || sizedTo == size || store.session(sessionId)?.canControl != true) return
        sizedTo = size
        commands.send(
            "session.resize",
            mapOf(
                "sessionId" to JsonPrimitive(sessionId),
                "cols" to JsonPrimitive(size.cols),
                "rows" to JsonPrimitive(size.rows),
                "widthPixels" to JsonPrimitive(size.cols * size.cellWidth),
                "heightPixels" to JsonPrimitive(size.rows * size.cellHeight),
                "cellWidthPixels" to JsonPrimitive(size.cellWidth),
                "cellHeightPixels" to JsonPrimitive(size.cellHeight),
                "surfaceGeneration" to JsonPrimitive(surfaceGeneration),
                "expectedRuntimeIncarnationId" to JsonPrimitive(incarnation),
                "subscriptionId" to JsonPrimitive(current.subscriptionId),
                "subscriptionGeneration" to JsonPrimitive(current.generation)
            )
        )
    }

    private suspend fun deliver(bytes: ByteArray) {
        for (wait in INPUT_BACKOFF_MS + listOf(0L)) {
            val current = link ?: return
            val result = runtime.input(sessionId, incarnation, current, bytes)
            if (result != Ffi.BUSY || wait == 0L) return
            delay(wait)
        }
    }

    private fun control(current: Link, fields: JsonObject) {
        if (link !== current) return
        val type = fields.string("type")
        when (type) {
            "term.closed" -> {
                leave(current)
                mutableState.value = State.Closed(fields.string("reason"))
            }

            "term.focusApplied", "term.focusRejected" -> {
                if (fields.string("requestId") != focus) return
                focus = null
                if (type == "term.focusApplied") {
                    focused = true
                    focusRetried = false
                    scheduleResize()
                } else if (!focusRetried) {
                    focusRetried = true
                    requestFocus()
                }
            }

            "term.resizeRejected" -> sizedTo = null
        }
    }

    private inner class Link(override val subscriptionId: String, override val generation: Long) : TerminalLink {
        @Volatile
        var active = true

        override fun onData(sequence: Long, bytes: ByteBuffer) {
            if (!active) return
            if (kernel.write(bytes, sequence) == TerminalKernel.OK) {
                changed()
            } else {
                active = false
                scope.launch { fail(this@Link, words.fellBehind) }
            }
        }

        override fun onControl(json: ByteArray) {
            if (!active) return
            val fields = runCatching {
                RuntimeJson.parseToJsonElement(json.decodeToString()) as? JsonObject
            }.getOrNull()
            if (fields == null) {
                scope.launch { fail(this@Link, words.invalidControl) }
                return
            }
            if (fields.string("type") == "term.resize") {
                val rows = fields.number("rows")
                val cols = fields.number("cols")
                val at = fields.number("atSequence")
                val applied = rows != null && cols != null && at != null &&
                    kernel.resize(
                        at,
                        rows.toInt(),
                        cols.toInt(),
                        fields.number("cellWidthPixels")?.toInt() ?: 0,
                        fields.number("cellHeightPixels")?.toInt() ?: 0
                    ) == TerminalKernel.OK
                if (applied) changed() else scope.launch { fail(this@Link, words.fellBehind) }
                return
            }
            scope.launch { control(this@Link, fields) }
        }

        override fun onCheckpoint(
            nextSequence: Long,
            rows: Int,
            cols: Int,
            console: Boolean,
            snapshot: ByteBuffer
        ): Int {
            if (!active) return Ffi.STALE_SUBSCRIPTION
            if (kernel.install(snapshot, nextSequence, rows, cols, console) != TerminalKernel.OK) {
                return Ffi.CHECKPOINT_REJECTED
            }
            changed()
            scope.launch {
                if (link !== this@Link) return@launch
                restored = true
                mutableState.value = State.Rendered
                refresh()
            }
            return Ffi.OK
        }

        override fun onConnectResult(result: Int) {
            scope.launch {
                if (result == Ffi.OK) {
                    if (link === this@Link && restored) mutableState.value = State.Rendered
                } else {
                    fail(this@Link, words.couldNotConnect)
                }
            }
        }
    }

    private companion object {
        val CONNECT_BACKOFF_MS = listOf(5L, 10L, 20L, 40L)
        val INPUT_BACKOFF_MS = listOf(5L, 10L, 20L, 40L, 80L, 160L)
        const val RESIZE_SETTLE_MS = 25L
        const val INPUT_CAPACITY = 256

        fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf {
            it.isString
        }?.contentOrNull

        fun JsonObject.number(key: String): Long? = (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.longOrNull
    }
}
