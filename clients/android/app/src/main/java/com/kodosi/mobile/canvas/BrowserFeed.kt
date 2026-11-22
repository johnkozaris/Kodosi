package com.kodosi.mobile.canvas

import android.graphics.BitmapFactory
import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
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

/**
 * A feed does its work on the browser's own thread, after each call of its view and in the order
 * of the calls and of the pipe's events.
 */
class BrowserFeed(private val pipes: BrowserPipes, internal val session: String, scope: CoroutineScope) :
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
    val undecodable: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val touches: StateFlow<Long>
        field = MutableStateFlow(0L)
    val copies: SharedFlow<String>
        field = MutableSharedFlow<String>(extraBufferCapacity = 8)
    private val work = CoroutineScope(scope.coroutineContext + pipes.dispatcher)
    private val link = pipes.link(session)
    private var joining: Job? = null
    private var active = false
    private var open = false
    private var watching = false
    private var behind = true
    private var last: Long? = null
    private var decoder: BrowserDecoder? = null
    private var size: JsonObject? = null
    private var sent: JsonObject? = null
    private var settling: Job? = null
    private var holding = false

    init {
        link.onFrame = ::receive
        link.onNews = ::heard
        pipes.execute { pipes.follow(this) }
    }

    /** A TextureView keeps its last frame after its decoder stops, so only a new surface is without a picture. */
    fun surface(surface: Surface?, width: Int, height: Int, density: Float) = pipes.execute {
        decoder?.close()
        picture.value = false
        undecodable.value = false
        decoder = surface?.let {
            BrowserDecoder(
                it,
                sized = { shown -> pictureSize.value = shown },
                refused = { refused -> pipes.execute { if (decoder === refused) refuse() } },
                lost = { failed -> pipes.execute { if (decoder === failed) restart() } }
            )
        }
        settle(width, height, density)
        restart()
    }

    private fun refuse() {
        decoder?.close()
        decoder = null
        undecodable.value = true
    }

    fun rendered() = pipes.execute { picture.value = true }

    fun resize(width: Int, height: Int, density: Float) = pipes.execute { settle(width, height, density) }

    /**
     * The page takes the shape of the view of the person who acted last, so a new size goes to
     * the host once the view settles, never while a button is held, and one that waits goes
     * right after the person's act: the act then reaches the page that the person saw. A pipe
     * that opens before the view has its size watches when the view has it, as the page takes
     * the shape of the watch at once.
     */
    private fun settle(width: Int, height: Int, density: Float) {
        if (width <= 0 || height <= 0) return
        size = fields(
            "width" to (width / density).toInt().coerceAtLeast(1),
            "height" to (height / density).toInt().coerceAtLeast(1),
            "scale" to density
        )
        settling?.cancel()
        if (open && !watching) {
            watch()
            return
        }
        settling = work.launch {
            delay(SETTLE_MS)
            sendSize()
        }
    }

    fun show(on: Boolean) = pipes.execute {
        if (active == on) return@execute
        active = on
        if (on) {
            join()
        } else {
            open = false
            watching = false
            joining?.cancel()
            link.detach()
            reconnecting.value = false
        }
    }

    internal fun chosen() {
        if (watching) watch()
    }

    fun input(value: JsonObject) = pipes.execute {
        touches.value++
        post(fields("type" to "input", "input" to value))
        if (value.string("kind") == "mouse") {
            when (value.string("action")) {
                "move" -> if (value.string("button") == "none") return@execute
                "press" -> holding = true
                "release" -> holding = false
            }
        }
        sendSize()
    }

    private fun sendSize() {
        val size = size ?: return
        if (holding || !watching || size == sent) return
        sent = size
        post(fields("type" to "size", "size" to size))
    }

    private fun join() {
        joining?.cancel()
        if (!active) return
        joining = work.launch {
            try {
                link.open()
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                failure.value = error.message
            }
        }
    }

    private fun heard(news: DocumentLink.News) {
        if (!active) return
        when (news) {
            DocumentLink.News.Open -> {
                failure.value = null
                behind = true
                last = null
                open = true
                watch()
            }

            is DocumentLink.News.Lost -> {
                open = false
                watching = false
                failure.value = news.reason
                reconnecting.value = true
            }

            DocumentLink.News.Closed -> {
                open = false
                watching = false
            }
        }
    }

    private fun watch() {
        val size = size ?: return
        sent = size
        watching = true
        post(fields("type" to "watch", "size" to size, "opened" to pipes.takeChoice(session)))
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
        reconnecting.value = false
    }

    /**
     * A view that waits for a key frame decodes no frame until it comes, so it asks once for each
     * break, and the host sends key frames at most once a second, as each costs every viewer.
     */
    private fun restart() {
        behind = true
        if (decoder != null) post(fields("type" to "keyframe"))
    }

    private fun post(value: JsonObject) {
        if (active) link.send(byteArrayOf(0) + value.toString().encodeToByteArray())
    }

    override fun close() = pipes.execute {
        active = false
        open = false
        watching = false
        pipes.unfollow(this)
        joining?.cancel()
        link.close()
        decoder?.close()
        decoder = null
    }

    private companion object {
        const val HEADER = 6
        const val KEY = 1
        const val SETTLE_MS = 250L
    }
}

/** The first decoder of the type may refuse the stream's size, and another may take it. */
internal fun deviceDecoders(size: IntSize): List<String> {
    val type = MediaFormat.MIMETYPE_VIDEO_AVC
    val format = MediaFormat.createVideoFormat(type, size.width, size.height)
    return MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos
        .filter { !it.isEncoder && !it.isAlias && type in it.supportedTypes }
        .sortedWith(compareBy({ !it.getCapabilitiesForType(type).isFormatSupported(format) }, { it.isSoftwareOnly }))
        .map { it.name }
}

internal class BrowserDecoder(
    private val surface: Surface,
    private val sized: (IntSize) -> Unit,
    private val refused: (BrowserDecoder) -> Unit,
    private val lost: (BrowserDecoder) -> Unit,
    private val decoders: (IntSize) -> List<String> = ::deviceDecoders
) : AutoCloseable {
    private val thread = HandlerThread("Kodosi browser decoder").apply { start() }
    private val handler = Handler(thread.looper)
    private val queued = AtomicInteger()
    private var codec: MediaCodec? = null
    private var name: String? = null
    private var configured: IntSize? = null
    private var closed = false
    private var waiting = true
    private var showed = false
    private var start: ByteArray? = null
    private val failures = mutableMapOf<String, Int>()
    private val buffers = ArrayDeque<Int>()
    private val frames = ArrayDeque<ByteArray>()

    fun frame(bytes: ByteArray, key: Boolean): Boolean {
        if (bytes.size > MAX_FRAME) return false
        if (queued.incrementAndGet() > 2) {
            queued.decrementAndGet()
            return false
        }
        return handler.post { take(bytes, key) }.also { if (!it) queued.decrementAndGet() }
    }

    private fun take(bytes: ByteArray, key: Boolean) {
        if (closed || (waiting && !key)) {
            queued.decrementAndGet()
            return
        }
        if (key) start = bytes
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
        } catch (_: NoDecoder) {
            queued.decrementAndGet()
            refused(this)
        } catch (_: Exception) {
            queued.decrementAndGet()
            fail()
        }
    }

    private fun create(size: IntSize) {
        val chosen = decoders(size).firstOrNull { failures.getOrDefault(it, 0) < TRIES } ?: throw NoDecoder()
        name = chosen
        showed = false
        configured = size
        val next = MediaCodec.createByCodecName(chosen)
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
                    runCatching { codec.releaseOutputBuffer(index, info.size > 0) }.onSuccess {
                        if (info.size > 0 && !showed) {
                            showed = true
                            failures.remove(chosen)
                        }
                    }
                }

                override fun onOutputFormatChanged(codec: MediaCodec, format: MediaFormat) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    format.shownSize()?.let(sized)
                }

                override fun onError(codec: MediaCodec, error: MediaCodec.CodecException) {
                    if (closed || this@BrowserDecoder.codec != codec) return
                    fail()
                }
            },
            handler
        )
        val video = requireNotNull(next.codecInfo.getCapabilitiesForType(TYPE).videoCapabilities)
        val width = (size.width + video.widthAlignment - 1) / video.widthAlignment * video.widthAlignment
        val height = (size.height + video.heightAlignment - 1) / video.heightAlignment * video.heightAlignment
        next.configure(
            MediaFormat.createVideoFormat(TYPE, width, height).apply {
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
            fail()
        }
    }

    /**
     * Some decoders fail on each frame, as the emulator's own decoder that may not read the app's
     * buffers, so one that fails before its first picture has a few tries with the stream's key
     * frame before the next decoder takes it.
     */
    private fun fail() {
        val failed = name.takeUnless { showed }
        reset()
        if (failed != null) {
            failures[failed] = failures.getOrDefault(failed, 0) + 1
            start?.let { frame(it, key = true) }
        }
        lost(this)
    }

    private fun reset() {
        val current = codec
        codec = null
        name = null
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

    private class NoDecoder : Exception()

    private companion object {
        const val TYPE = MediaFormat.MIMETYPE_VIDEO_AVC
        const val TRIES = 3
        const val MAX_FRAME = 16 * 1024 * 1024
    }
}
