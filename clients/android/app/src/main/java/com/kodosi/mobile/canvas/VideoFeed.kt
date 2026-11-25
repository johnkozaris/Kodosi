package com.kodosi.mobile.canvas

import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import androidx.compose.ui.unit.IntSize
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

// The part of a stream's source that a frame shows, in the source's points.
data class FrameRegion(val x: Float, val y: Float, val width: Float, val height: Float)

// A feed of a video pipe: the shared browser's or the screen's. All mutable state is confined to the
// single-threaded VideoPipes executor to preserve ordering between UI calls and pipe events.
// `decoders` tells the host what this device decodes.
abstract class VideoFeed(
    private val pipes: VideoPipes,
    internal val session: String,
    key: String,
    scope: CoroutineScope,
    private val decoders: List<JsonObject>
) : AutoCloseable {
    val picture: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val pictureSize: StateFlow<IntSize?>
        field = MutableStateFlow<IntSize?>(null)

    // The part of the source that the newest frame shows, or null for all of it.
    val region: StateFlow<FrameRegion?>
        field = MutableStateFlow<FrameRegion?>(null)
    val reconnecting: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val failure: StateFlow<String?>
        field = MutableStateFlow<String?>(null)
    val undecodable: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val touches: StateFlow<Long>
        field = MutableStateFlow(0L)
    protected val work = CoroutineScope(scope.coroutineContext + pipes.dispatcher)
    private val link = pipes.link(session, key)
    private var joining: Job? = null
    private var active = false
    private var open = false
    protected var watching = false
        private set
    private var behind = true
    private var last: Long? = null
    private var decoder: VideoDecoder? = null
    private var size: JsonObject? = null
    private var sent: JsonObject? = null
    private var settling: Job? = null
    private var holding = false

    init {
        link.onFrame = ::receive
        link.onNews = ::heard
        pipes.execute { pipes.follow(this) }
    }

    // A message of the stream, on the pipes' thread.
    protected abstract fun take(message: JsonObject)

    // A new pipe is a new seat at the stream, which starts as it is.
    protected open fun opened() = Unit

    // Runs on the pipes' thread, after what is already queued there.
    protected fun execute(job: () -> Unit) = pipes.execute(job)

    // The surface keeps its last picture while a decoder starts again, so only a new surface clears
    // the picture.
    fun surface(surface: Surface, width: Int, height: Int, density: Float) = pipes.execute {
        decoder?.close()
        picture.value = false
        undecodable.value = false
        decoder = VideoDecoder(
            surface,
            sized = { shown -> pictureSize.value = shown },
            rendered = { shown -> pipes.execute { if (decoder === shown) picture.value = true } },
            refused = { refused -> pipes.execute { if (decoder === refused) refuse() } },
            lost = { failed -> pipes.execute { if (decoder === failed) restart() } }
        )
        settle(width, height, density)
        restart()
    }

    // A surface must have no producer once its view lets go of it, so this waits until the decoder
    // lets go of it.
    fun detach() {
        val detached = CountDownLatch(1)
        pipes.execute {
            decoder?.close()
            decoder = null
            picture.value = false
            undecodable.value = false
            detached.countDown()
        }
        detached.await(DETACH_MS, TimeUnit.MILLISECONDS)
    }

    private fun refuse() {
        decoder?.close()
        decoder = null
        undecodable.value = true
    }

    fun resize(width: Int, height: Int, density: Float) = pipes.execute { settle(width, height, density) }

    // The stream's size follows the last active viewer, so debounce resizes and hold them during mouse
    // drags; pending sizes flush after input so clicks hit the layout the user saw. If the pipe opened
    // before layout completed, send the initial watch as soon as we have a size.
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
            settled(width, height)
        }
    }

    // The view's size in pixels settled, after a resize.
    protected open fun settled(width: Int, height: Int) = Unit

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

    // Whether the stream plays this viewer's input.
    protected open val takesInput: Boolean get() = true

    fun input(value: JsonObject) = pipes.execute {
        if (!takesInput) return@execute
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
                opened()
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
        post(
            fields(
                "type" to "watch",
                "size" to size,
                "decoders" to decoders,
                "opened" to pipes.takeChoice(session)
            )
        )
        watched()
    }

    // The stream heard this viewer's watch.
    protected open fun watched() = Unit

    private fun receive(bytes: ByteArray) {
        if (!active || bytes.isEmpty()) return
        if (bytes[0] == 0.toByte()) {
            runCatching {
                RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as? JsonObject
            }.getOrNull()?.let(::take)
            return
        }
        if (bytes[0] != 1.toByte() || bytes.size <= HEADER) return
        val id = ByteBuffer.wrap(bytes, 1, 4).order(ByteOrder.BIG_ENDIAN).int.toLong() and 0xffffffffL
        val flags = bytes[5].toInt()
        val key = flags and KEY != 0
        post(fields("type" to "ack", "frame" to id))
        val codec = VideoCodec.of(bytes[6].toInt()) ?: return
        var start = HEADER
        var shows: FrameRegion? = null
        if (flags and REGION != 0) {
            if (bytes.size <= HEADER + REGION_BYTES) return
            val values = ByteBuffer.wrap(bytes, HEADER, REGION_BYTES).order(ByteOrder.BIG_ENDIAN)
            shows = FrameRegion(values.float, values.float, values.float, values.float)
            start += REGION_BYTES
        }
        if (!key && !behind && id != (last?.plus(1)?.and(0xffffffffL))) restart()
        last = id
        if (behind && !key) return
        if (decoder?.frame(bytes.copyOfRange(start, bytes.size), key, codec) != true) {
            restart()
            return
        }
        region.value = shows
        behind = false
        reconnecting.value = false
    }

    // Drop frames until the next keyframe; request one per break (the host rate-limits keyframes to
    // 1/s because they cost bandwidth for all viewers).
    private fun restart() {
        behind = true
        if (decoder != null) post(fields("type" to "keyframe"))
    }

    protected fun post(value: JsonObject) {
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
        const val HEADER = 7
        const val REGION_BYTES = 16
        const val KEY = 1
        const val REGION = 4
        const val SETTLE_MS = 250L
        const val DETACH_MS = 1000L
    }
}

// Return ranked decoders because the default codec can reject a stream size another accepts.
internal fun deviceDecoders(codec: VideoCodec, size: IntSize): List<String> {
    val type = codec.mime
    val format = MediaFormat.createVideoFormat(type, size.width, size.height)
    return MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos
        .filter { !it.isEncoder && !it.isAlias && type in it.supportedTypes }
        .sortedWith(compareBy({ !it.getCapabilitiesForType(type).isFormatSupported(format) }, { it.isSoftwareOnly }))
        .map { it.name }
}

internal class VideoDecoder(
    private val surface: Surface,
    private val sized: (IntSize) -> Unit,
    private val rendered: (VideoDecoder) -> Unit,
    private val refused: (VideoDecoder) -> Unit,
    private val lost: (VideoDecoder) -> Unit,
    private val decoders: (VideoCodec, IntSize) -> List<String> = ::deviceDecoders
) : AutoCloseable {
    private val thread = HandlerThread("Kodosi video decoder").apply { start() }
    private val handler = Handler(thread.looper)
    private val queued = AtomicInteger()
    private var codec: MediaCodec? = null
    private var name: String? = null
    private var configured: Stream? = null
    private var closed = false
    private var waiting = true
    private var showed = false
    private var start: Keyframe? = null
    private val failures = mutableMapOf<String, Int>()
    private val buffers = ArrayDeque<Int>()
    private val frames = ArrayDeque<ByteArray>()

    fun frame(bytes: ByteArray, key: Boolean, codec: VideoCodec): Boolean {
        if (bytes.size > MAX_FRAME) return false
        if (queued.incrementAndGet() > 2) {
            queued.decrementAndGet()
            return false
        }
        return handler.post { take(bytes, key, codec) }.also { if (!it) queued.decrementAndGet() }
    }

    private fun take(bytes: ByteArray, key: Boolean, codec: VideoCodec) {
        if (closed || (waiting && !key)) {
            queued.decrementAndGet()
            return
        }
        if (key) start = Keyframe(bytes, codec)
        try {
            val stream = if (key) codec.pictureSize(bytes)?.let { Stream(codec, it) } else null
            if (stream != null && stream != configured) {
                reset()
                create(stream)
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

    private fun create(stream: Stream) {
        val (type, size) = stream
        val chosen = decoders(type, size).firstOrNull { failures.getOrDefault(it, 0) < TRIES } ?: throw NoDecoder()
        name = chosen
        showed = false
        configured = stream
        val next = MediaCodec.createByCodecName(chosen)
        codec = next
        next.setCallback(
            object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(codec: MediaCodec, index: Int) {
                    if (closed || this@VideoDecoder.codec != codec) return
                    buffers.addLast(index)
                    feed()
                }

                // A timestamp of now shows the picture at the next refresh of the display.
                override fun onOutputBufferAvailable(codec: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    if (closed || this@VideoDecoder.codec != codec) return
                    val shows = info.size > 0
                    runCatching {
                        if (shows) {
                            codec.releaseOutputBuffer(
                                index,
                                System.nanoTime()
                            )
                        } else {
                            codec.releaseOutputBuffer(index, false)
                        }
                    }.onSuccess {
                        if (shows && !showed) {
                            showed = true
                            failures.remove(chosen)
                            rendered(this@VideoDecoder)
                        }
                    }
                }

                override fun onOutputFormatChanged(codec: MediaCodec, format: MediaFormat) {
                    if (closed || this@VideoDecoder.codec != codec) return
                    format.shownSize()?.let(sized)
                }

                override fun onError(codec: MediaCodec, error: MediaCodec.CodecException) {
                    if (closed || this@VideoDecoder.codec != codec) return
                    fail()
                }
            },
            handler
        )
        val video = requireNotNull(next.codecInfo.getCapabilitiesForType(type.mime).videoCapabilities)
        val width = (size.width + video.widthAlignment - 1) / video.widthAlignment * video.widthAlignment
        val height = (size.height + video.heightAlignment - 1) / video.heightAlignment * video.heightAlignment
        next.configure(
            MediaFormat.createVideoFormat(type.mime, width, height).apply {
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, MAX_FRAME)
                setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                val supported = next.supportedVendorParameters.toSet()
                VENDOR_LOW_LATENCY.filterKeys { it in supported }.forEach { (key, value) -> setInteger(key, value) }
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

    // Some decoders (such as the Android emulator's) fail on every frame. A codec that fails before its
    // first picture gets up to TRIES tries with the cached keyframe before it is blacklisted and the
    // next one takes over.
    private fun fail() {
        val failed = name.takeUnless { showed }
        reset()
        if (failed != null) {
            failures[failed] = failures.getOrDefault(failed, 0) + 1
            start?.let { frame(it.bytes, key = true, it.codec) }
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

    // Returns once the codec let go of the surface, or after CLOSE_MS.
    override fun close() {
        val released = CountDownLatch(1)
        handler.post {
            closed = true
            reset()
            thread.quitSafely()
            released.countDown()
        }
        released.await(CLOSE_MS, TimeUnit.MILLISECONDS)
    }

    // MediaFormat crop bounds are inclusive, hence + 1.
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

    private data class Stream(val codec: VideoCodec, val size: IntSize)

    private class Keyframe(val bytes: ByteArray, val codec: VideoCodec)

    private companion object {
        const val TRIES = 3
        const val MAX_FRAME = 16 * 1024 * 1024
        const val CLOSE_MS = 500L

        // Switches of each vendor's decoders for the low latency that KEY_LOW_LATENCY asks of them.
        val VENDOR_LOW_LATENCY = mapOf(
            "vendor.qti-ext-dec-picture-order.enable" to 1,
            "vendor.qti-ext-dec-low-latency.enable" to 1,
            "vendor.rtc-ext-dec-low-latency.enable" to 1,
            "vendor.low-latency.enable" to 1,
            "vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-req" to 1,
            "vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-rdy" to -1
        )
    }
}
