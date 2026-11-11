package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.number
import java.io.ByteArrayOutputStream
import java.util.Base64

class DocumentLink(
    private val commands: CommandSink,
    private val session: String,
    private val key: String,
    private val onClose: (DocumentLink) -> Unit
) {
    private var pipe: String? = null
    private var next = 0L
    private var filling = ByteArrayOutputStream()
    var onFrame: (ByteArray) -> Unit = {}
    var onLost: () -> Unit = {}

    suspend fun open() {
        detach()
        val opening = UuidV7.generate()
        pipe = opening
        next = 0
        filling.reset()
        try {
            commands.request("document.open", fields("sessionId" to session, "pipe" to opening, "key" to key))
        } catch (failure: Exception) {
            if (pipe == opening) detach()
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
                lost()
                return
            }
            offset = end
        }
    }

    fun receive(event: RuntimeEvent) {
        val pipe = pipe ?: return
        if (event.string("pipe") != pipe) return
        val data = event.string("data")?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
        if (event.type != "document.frame" || event.payload.number("sequence") != next ||
            event.bool("more") == null || data == null || filling.size().toLong() + data.size > MAX_FRAME_BYTES
        ) {
            lost()
            return
        }
        next++
        filling.write(data)
        if (event.bool("more") == false) {
            val frame = filling.toByteArray()
            filling = ByteArrayOutputStream()
            onFrame(frame)
        }
    }

    private fun lost() {
        detach()
        onLost()
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
        onLost = {}
        onClose(this)
    }

    private companion object {
        const val MAX_FRAME_BYTES = 64 * 1024 * 1024
    }
}
