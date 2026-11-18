package com.kodosi.mobile.canvas

import android.graphics.BitmapFactory
import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.util.Base64
import android.view.Surface
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.IntSize
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

class BrowserFeed(private val store: CanvasStore, private val session: String, private val scope: CoroutineScope) :
    AutoCloseable {
    val window: StateFlow<JsonObject>
        field = MutableStateFlow(fields())
    val icons: StateFlow<Map<String, ImageBitmap>>
        field = MutableStateFlow(emptyMap<String, ImageBitmap>())
    val picture: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val pictureSize: StateFlow<IntSize?>
        field = MutableStateFlow<IntSize?>(null)
    val reconnecting: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val failure: StateFlow<String?>
        field = MutableStateFlow<String?>(null)
    val touches: StateFlow<Long>
        field = MutableStateFlow(0L)
    val copies: SharedFlow<String>
        field = MutableSharedFlow<String>(extraBufferCapacity = 8)
    private val link = store.link(session, "browser")
    private var joining: Job? = null
    private var active = false
    private var behind = true
    private var last: Long? = null
    private var decoder: BrowserDecoder? = null
    private var size = fields("width" to 360, "height" to 640, "scale" to 1)
    private var sent: JsonObject? = null
    private var settling: Job? = null
    private var holding = false
    private var keyFrameRequested = 0L
    private var keyFrameRequest: Job? = null
    private var awaitingFrames: Job? = null

    init {
        link.onFrame = ::receive
        link.onLost = { join() }
    }

    /** A TextureView keeps its last frame after its decoder stops, so only a new surface is without a picture. */
    fun surface(surface: Surface?, width: Int, height: Int, density: Float) {
        decoder?.close()
        picture.value = false
        decoder = surface?.let {
            BrowserDecoder(it, { shown -> pictureSize.value = shown }) { failed ->
                scope.launch { if (decoder === failed) restart() }
            }
        }
        resize(width, height, density)
        restart()
    }

    fun rendered() {
        picture.value = true
    }

    /**
     * The page takes the shape of the view of the person who acted last, so a new size goes to
     * the host once the view settles, never while a button is held, and one that waits goes
     * right after the person's act: the act then reaches the page that the person saw.
     */
    fun resize(width: Int, height: Int, density: Float) {
        if (width <= 0 || height <= 0) return
        size = fields(
            "width" to (width / density).toInt().coerceAtLeast(1),
            "height" to (height / density).toInt().coerceAtLeast(1),
            "scale" to density
        )
        settling?.cancel()
        settling = scope.launch {
            delay(SETTLE_MS)
            sendSize()
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
            stopAwaitingFrames()
        }
    }

    fun input(value: JsonObject) {
        touches.value++
        post(fields("type" to "input", "input" to value))
        if (value.string("kind") == "mouse") {
            when (value.string("action")) {
                "move" -> if (value.string("button") == "none") return
                "press" -> holding = true
                "release" -> holding = false
            }
        }
        sendSize()
    }

    private fun sendSize() {
        if (holding || size == sent) return
        sent = size
        post(fields("type" to "size", "size" to size))
    }

    private fun join() {
        joining?.cancel()
        if (!active) return
        awaitFrames()
        joining = scope.launch {
            var wait = 1000L
            while (active) {
                try {
                    link.open()
                    failure.value = null
                    behind = true
                    last = null
                    sent = size
                    post(fields("type" to "watch", "size" to size))
                    return@launch
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (error: Exception) {
                    failure.value = error.message
                    delay(wait)
                    wait = minOf(wait * 2, 30_000)
                }
            }
        }
    }

    private fun awaitFrames() {
        if (awaitingFrames != null) return
        awaitingFrames = scope.launch {
            delay(RECONNECTING_AFTER_MS)
            reconnecting.value = true
        }
    }

    private fun stopAwaitingFrames() {
        awaitingFrames?.cancel()
        awaitingFrames = null
        reconnecting.value = false
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

    private fun receive(bytes: ByteArray) {
        if (!active || bytes.isEmpty()) return
        if (bytes[0] == 0.toByte()) {
            runCatching {
                RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as? JsonObject
            }.getOrNull()?.let {
                window.value = it
                keepIcons(it)
                it.string("copied")?.let(copies::tryEmit)
            }
            return
        }
        if (bytes[0] != 1.toByte() || bytes.size <= HEADER) return
        val id = ByteBuffer.wrap(bytes, 1, 4).order(ByteOrder.BIG_ENDIAN).int.toLong() and 0xffffffffL
        val key = bytes[5].toInt() and KEY != 0
        post(fields("type" to "ack", "frame" to id))
        if (!key && !behind && id != (last?.plus(1)?.and(0xffffffffL))) restart()
        last = id
        if (behind && !key) return
        if (decoder?.frame(bytes.copyOfRange(HEADER, bytes.size), key) != true) {
            restart()
            return
        }
        behind = false
        stopAwaitingFrames()
    }

    /** A key frame costs the host and the link many times the bits of the frames between. */
    private fun restart() {
        behind = true
        if (decoder == null || keyFrameRequest?.isActive == true) return
        val wait = keyFrameRequested + KEY_FRAME_REQUEST_GAP_MS - SystemClock.elapsedRealtime()
        if (wait <= 0) {
            requestKeyFrame()
        } else {
            keyFrameRequest = scope.launch {
                delay(wait)
                if (behind && decoder != null) requestKeyFrame()
            }
        }
    }

    private fun requestKeyFrame() {
        keyFrameRequested = SystemClock.elapsedRealtime()
        post(fields("type" to "keyframe"))
    }

    private fun post(value: JsonObject) {
        if (active) link.send(byteArrayOf(0) + value.toString().encodeToByteArray())
    }

    override fun close() {
        active = false
        joining?.cancel()
        keyFrameRequest?.cancel()
        awaitingFrames?.cancel()
        link.close()
        decoder?.close()
        decoder = null
    }

    private companion object {
        const val HEADER = 14
        const val KEY = 1
        const val SETTLE_MS = 250L
        const val KEY_FRAME_REQUEST_GAP_MS = 500L
        const val RECONNECTING_AFTER_MS = 1500L
    }
}

internal class BrowserDecoder(
    private val surface: Surface,
    private val sized: (IntSize) -> Unit,
    private val lost: (BrowserDecoder) -> Unit
) : AutoCloseable {
    private val thread = HandlerThread("Kodosi browser decoder").apply { start() }
    private val handler = Handler(thread.looper)
    private val queued = AtomicInteger()
    private var codec: MediaCodec? = null
    private var configured: IntSize? = null
    private var closed = false
    private var waiting = true
    private val buffers = ArrayDeque<Int>()
    private val frames = ArrayDeque<ByteArray>()

    fun frame(bytes: ByteArray, key: Boolean): Boolean {
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
                val size = if (key) H264.pictureSize(bytes) else null
                if (size != null && size != configured) {
                    reset()
                    create(size)
                }
                checkNotNull(codec)
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

    /** The first decoder of the type may refuse the stream's size, and another may take it. */
    private fun create(size: IntSize) {
        val type = MediaFormat.MIMETYPE_VIDEO_AVC
        val next = MediaCodecList(MediaCodecList.REGULAR_CODECS)
            .findDecoderForFormat(MediaFormat.createVideoFormat(type, size.width, size.height))
            ?.let(MediaCodec::createByCodecName) ?: MediaCodec.createDecoderByType(type)
        codec = next
        configured = size
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

                override fun onOutputFormatChanged(codec: MediaCodec, format: MediaFormat) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    format.shownSize()?.let(sized)
                }

                override fun onError(codec: MediaCodec, error: MediaCodec.CodecException) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    reset()
                    lost(this@BrowserDecoder)
                }
            },
            handler
        )
        val video = requireNotNull(next.codecInfo.getCapabilitiesForType(type).videoCapabilities)
        val width = (size.width + video.widthAlignment - 1) / video.widthAlignment * video.widthAlignment
        val height = (size.height + video.heightAlignment - 1) / video.heightAlignment * video.heightAlignment
        next.configure(
            MediaFormat.createVideoFormat(type, width, height).apply {
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
        configured = null
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

    /** A decoder gives the visible part of its pictures as a crop with inclusive edges. */
    private fun MediaFormat.shownSize(): IntSize? {
        fun side(whole: String, start: String, end: String) = if (containsKey(start) && containsKey(end)) {
            getInteger(end) - getInteger(start) + 1
        } else {
            getInteger(whole, 0)
        }
        val width = side(MediaFormat.KEY_WIDTH, MediaFormat.KEY_CROP_LEFT, MediaFormat.KEY_CROP_RIGHT)
        val height = side(MediaFormat.KEY_HEIGHT, MediaFormat.KEY_CROP_TOP, MediaFormat.KEY_CROP_BOTTOM)
        return IntSize(width, height).takeIf { width > 0 && height > 0 }
    }

    private companion object {
        const val MAX_FRAME = 16 * 1024 * 1024
    }
}
