package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.number
import java.io.ByteArrayOutputStream
import java.util.Base64

/** The runtime keeps the pipe of a view and opens it again after each loss, until the view closes it. */
class DocumentLink(
    private val commands: CommandSink,
    val session: String,
    private val key: String,
    private val onClose: (DocumentLink) -> Unit
) {
    sealed interface News {
        /** The pipe is open, again after each loss: the view sends its first message. */
        data object Open : News

        /** The pipe stayed lost for a while, for the reason that the runtime gives. */
        data class Lost(val reason: String?) : News

        /** The terminal of the pipe is gone. */
        data object Closed : News
    }

    private var pipe: String? = null

    /** The next part, or none while the pipe has not opened. */
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
            commands.request("document.open", fields("sessionId" to session, "pipe" to opening, "key" to key))
        } catch (failure: Exception) {
            if (pipe == opening) pipe = null
            throw failure
        }
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

    /** A part that went missing breaks the frames, so the pipe starts again, and the parts on their way go nowhere. */
    private fun renew(pipe: String) {
        filling.reset()
        next = null
        commands.send("document.close", fields("pipe" to pipe))
        if (commands.send("document.open", fields("sessionId" to session, "pipe" to pipe, "key" to key)) != Ffi.OK) {
            this.pipe = null
            onNews(News.Closed)
        }
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
