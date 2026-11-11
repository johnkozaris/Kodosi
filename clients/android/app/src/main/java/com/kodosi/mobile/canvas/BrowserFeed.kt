package com.kodosi.mobile.canvas

import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.number
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

class BrowserFeed(private val store: CanvasStore, private val session: String, private val scope: CoroutineScope) :
    AutoCloseable {
    private val mutableWindow = MutableStateFlow(fields())
    val window: StateFlow<JsonObject> = mutableWindow
    private val mutablePicture = MutableStateFlow(false)
    val picture: StateFlow<Boolean> = mutablePicture
    private val mutableTouches = MutableStateFlow(0L)
    val touches: StateFlow<Long> = mutableTouches
    private val link = store.link(session, "browser")
    private var joining: Job? = null
    private var active = false
    private var behind = true
    private var last: Long? = null
    private var decoder: BrowserDecoder? = null
    private var size = fields("width" to 360, "height" to 640, "scale" to 1)

    init {
        link.onFrame = ::receive
        link.onLost = { join() }
    }

    fun surface(surface: Surface?, width: Int, height: Int, density: Float) {
        decoder?.close()
        mutablePicture.value = false
        decoder = surface?.let {
            BrowserDecoder(it) { failed ->
                scope.launch {
                    if (decoder === failed) {
                        mutablePicture.value = false
                        restart()
                    }
                }
            }
        }
        resize(width, height, density)
        restart()
    }

    fun rendered() {
        if (active) mutablePicture.value = true
    }

    fun resize(width: Int, height: Int, density: Float) {
        if (width <= 0 || height <= 0) return
        val next = fields(
            "width" to (width / density).toInt().coerceAtLeast(1),
            "height" to (height / density).toInt().coerceAtLeast(1),
            "scale" to density
        )
        if (next != size) {
            size = next
            post(fields("type" to "size", "size" to size))
        }
    }

    fun show(on: Boolean) {
        if (active == on) return
        active = on
        if (on) {
            join()
        } else {
            joining?.cancel()
            link.detach()
            mutablePicture.value = false
        }
    }

    fun input(value: JsonObject) {
        mutableTouches.value++
        post(fields("type" to "size", "size" to size))
        post(fields("type" to "input", "input" to value))
    }

    private fun join() {
        joining?.cancel()
        if (!active) return
        mutablePicture.value = false
        joining = scope.launch {
            var wait = 1000L
            while (active) {
                try {
                    link.open()
                    behind = true
                    last = null
                    post(fields("type" to "watch", "size" to size))
                    return@launch
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (_: Exception) {
                    delay(wait)
                    wait = minOf(wait * 2, 30_000)
                }
            }
        }
    }

    private fun receive(bytes: ByteArray) {
        if (!active || bytes.isEmpty()) return
        if (bytes[0] == 0.toByte()) {
            runCatching {
                RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as? JsonObject
            }.getOrNull()?.let {
                mutableWindow.value =
                    it
            }
            return
        }
        if (bytes[0] != 1.toByte() || bytes.size <= HEADER) return
        val id = ByteBuffer.wrap(bytes, 1, 4).order(ByteOrder.BIG_ENDIAN).int.toLong() and 0xffffffffL
        val key = bytes[5] != 0.toByte()
        post(fields("type" to "ack", "frame" to id))
        if (!key && !behind && id != (last?.plus(1)?.and(0xffffffffL))) restart()
        last = id
        if (behind && !key) return
        val width = (window.value.number("width") ?: 640).toInt().coerceIn(1, 8192)
        val height = (window.value.number("height") ?: 480).toInt().coerceIn(1, 8192)
        if (decoder?.frame(bytes.copyOfRange(HEADER, bytes.size), width, height, key) != true) {
            restart()
            return
        }
        behind = false
    }

    private fun restart() {
        behind = true
        post(fields("type" to "keyframe"))
    }

    private fun post(value: JsonObject) {
        if (active) link.send(byteArrayOf(0) + value.toString().encodeToByteArray())
    }

    override fun close() {
        active = false
        joining?.cancel()
        link.close()
        decoder?.close()
        decoder = null
    }

    private companion object {
        const val HEADER = 14
    }
}

internal class BrowserDecoder(private val surface: Surface, private val lost: (BrowserDecoder) -> Unit) :
    AutoCloseable {
    private val thread = HandlerThread("Kodosi browser decoder").apply { start() }
    private val handler = Handler(thread.looper)
    private val queued = AtomicInteger()
    private var codec: MediaCodec? = null
    private var closed = false
    private var waiting = true
    private val buffers = ArrayDeque<Int>()
    private val frames = ArrayDeque<ByteArray>()

    fun frame(bytes: ByteArray, width: Int, height: Int, key: Boolean): Boolean {
        if (bytes.size > MAX_FRAME) return false
        if (queued.incrementAndGet() > 2) {
            queued.decrementAndGet()
            return false
        }
        return handler.post {
            if (closed || (waiting && !key)) {
                queued.decrementAndGet()
                return@post
            }
            try {
                if (codec == null) create(width, height)
                waiting = false
                frames.addLast(bytes)
                feed()
            } catch (_: Exception) {
                queued.decrementAndGet()
                reset()
                lost(this)
            }
        }.also { if (!it) queued.decrementAndGet() }
    }

    private fun create(width: Int, height: Int) {
        val next = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        codec = next
        next.setCallback(
            object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(codec: MediaCodec, index: Int) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    buffers.addLast(index)
                    feed()
                }

                override fun onOutputBufferAvailable(codec: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    runCatching { codec.releaseOutputBuffer(index, info.size > 0) }
                }

                override fun onOutputFormatChanged(codec: MediaCodec, format: MediaFormat) = Unit

                override fun onError(codec: MediaCodec, error: MediaCodec.CodecException) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    reset()
                    lost(this@BrowserDecoder)
                }
            },
            handler
        )
        val video =
            requireNotNull(next.codecInfo.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).videoCapabilities)
        // The window supplies CSS dimensions; the stream supplies its coded size.
        val initialWidth = (width + video.widthAlignment - 1) / video.widthAlignment * video.widthAlignment
        val initialHeight = (height + video.heightAlignment - 1) / video.heightAlignment * video.heightAlignment
        next.configure(
            MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, initialWidth, initialHeight).apply {
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, MAX_FRAME)
                setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
            },
            surface,
            null,
            0
        )
        next.start()
    }

    private fun feed() {
        val codec = codec ?: return
        try {
            while (frames.isNotEmpty() && buffers.isNotEmpty()) {
                val index = buffers.removeFirst()
                val frame = frames.removeFirst()
                queued.decrementAndGet()
                val input = codec.getInputBuffer(index) ?: throw IllegalStateException()
                if (frame.size > input.capacity()) throw IllegalStateException()
                input.clear()
                input.put(frame)
                codec.queueInputBuffer(index, 0, frame.size, System.nanoTime() / 1000, 0)
            }
        } catch (_: Exception) {
            reset()
            lost(this)
        }
    }

    private fun reset() {
        val current = codec
        codec = null
        runCatching { current?.stop() }
        runCatching { current?.release() }
        queued.addAndGet(-frames.size)
        frames.clear()
        buffers.clear()
        waiting = true
    }

    override fun close() {
        handler.post {
            closed = true
            reset()
            thread.quitSafely()
        }
    }

    private companion object {
        const val MAX_FRAME = 16 * 1024 * 1024
    }
}
