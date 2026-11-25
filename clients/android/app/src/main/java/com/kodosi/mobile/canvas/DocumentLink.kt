package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.LostAnswer
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.number
import java.io.ByteArrayOutputStream
import java.util.Base64

// The runtime owns each pipe and reconnects it after drops until closed. Each DocumentLink is
// single-threaded: main thread for documents, the VideoPipes executor for the browser and the screen.
class DocumentLink(
    private val commands: CommandSink,
    val session: String,
    private val key: String,
    private val onPipe: () -> Unit = {},
    private val onClose: (DocumentLink) -> Unit
) {
    sealed interface News {
        // Emitted on initial open and after each reconnect; the view must resend its handshake.
        data object Open : News

        data class Lost(val reason: String?) : News

        // Emitted when the owning terminal is gone and the pipe will not reconnect.
        data object Closed : News
    }

    var pipe: String? = null
        private set(value) {
            field = value
            onPipe()
        }

    private var registering: String? = null
    private var next: Long? = null
    private var filling = ByteArrayOutputStream()
    var onFrame: (ByteArray) -> Unit = {}
    var onNews: (News) -> Unit = {}

    suspend fun open() {
        detach()
        val opening = UuidV7.generate()
        pipe = opening
        next = null
        filling.reset()
        try {
            register(opening)
        } catch (failure: Exception) {
            if (pipe == opening) pipe = null
            throw failure
        }
    }

    // Restart active pipes after event loss; pipes still registering retry via LostAnswer.
    fun renewAfterLoss() {
        val pipe = pipe ?: return
        if (pipe != registering) renew(pipe)
    }

    fun send(bytes: ByteArray) {
        val pipe = pipe ?: return
        var offset = 0
        while (offset < bytes.size) {
            val end = minOf(offset + CanvasStore.PART_BYTES, bytes.size)
            val result = commands.send(
                "document.send",
                fields(
                    "pipe" to pipe,
                    "more" to (end < bytes.size),
                    "data" to Base64.getEncoder().encodeToString(bytes.copyOfRange(offset, end))
                )
            )
            if (result != Ffi.OK) {
                renew(pipe)
                return
            }
            offset = end
        }
    }

    fun receive(event: RuntimeEvent) {
        val pipe = pipe ?: return
        if (event.string("pipe") != pipe) return
        when (event.type) {
            "document.opened" -> {
                filling.reset()
                next = 0
                onNews(News.Open)
            }

            "document.lost" -> {
                filling.reset()
                onNews(News.Lost(event.string("message")))
            }

            "document.closed" -> {
                this.pipe = null
                filling.reset()
                onNews(News.Closed)
            }

            "document.frame" -> frame(pipe, event)
        }
    }

    private fun frame(pipe: String, event: RuntimeEvent) {
        val expected = next ?: return
        val data = event.string("data")?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
        val more = event.bool("more")
        if (event.payload.number("sequence") != expected || more == null || data == null ||
            filling.size().toLong() + data.size > MAX_FRAME_BYTES
        ) {
            renew(pipe)
            return
        }
        next = expected + 1
        filling.write(data)
        if (!more) {
            val frame = filling.toByteArray()
            filling = ByteArrayOutputStream()
            onFrame(frame)
        }
    }

    // If event loss drops the open reply, the runtime's pipe state is unknown; close and retry.
    private suspend fun register(pipe: String) {
        registering = pipe
        try {
            while (true) {
                try {
                    commands.request("document.open", fields("sessionId" to session, "pipe" to pipe, "key" to key))
                    return
                } catch (lost: LostAnswer) {
                    if (this.pipe != pipe) throw lost
                    shut(pipe)
                }
            }
        } finally {
            if (registering == pipe) registering = null
        }
    }

    private fun renew(pipe: String) {
        shut(pipe)
        if (commands.send("document.open", fields("sessionId" to session, "pipe" to pipe, "key" to key)) != Ffi.OK) {
            this.pipe = null
            onNews(News.Closed)
        }
    }

    // Clear next so in-flight frames are ignored until the pipe reopens.
    private fun shut(pipe: String) {
        filling.reset()
        next = null
        commands.send("document.close", fields("pipe" to pipe))
    }

    fun detach() {
        val current = pipe ?: return
        pipe = null
        filling.reset()
        commands.send("document.close", fields("pipe" to current))
    }

    fun close() {
        detach()
        onFrame = {}
        onNews = {}
        onClose(this)
    }

    private companion object {
        const val MAX_FRAME_BYTES = 64 * 1024 * 1024
    }
}
