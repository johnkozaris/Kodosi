package com.kodosi.mobile.canvas

import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.os.PowerManager
import android.view.Surface
import androidx.compose.ui.unit.IntSize
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import java.nio.ByteBuffer
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlin.time.Duration.Companion.seconds
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

// The part of a stream's source that a frame shows, in the source's points.
data class FrameRegion(val x: Float, val y: Float, val width: Float, val height: Float)

// A feed of a video pipe: the shared browser's or the screen's. The runtime hands its messages and frames over on the
// pipe's own thread: a frame goes straight to the decoder there, and the rest goes on to the single-threaded VideoPipes
// executor, in pipe order, which keeps all other state. `decoders` tells the host what this device decodes.
abstract class VideoFeed(
    private val pipes: VideoPipes,
    internal val session: String,
    key: String,
    scope: CoroutineScope,
    private val decoders: List<JsonObject>
) : AutoCloseable {
    // The surface shows a picture, which stays on it while the stream stops.
    val picture: StateFlow<Boolean>
        field = MutableStateFlow(false)

    // Pictures come again since the stream last stopped, so the picture is the stream's own.
    val live: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val pictureSize: StateFlow<IntSize?>
        field = MutableStateFlow<IntSize?>(null)

    // The part of the source that the newest frame shows, or null for all of it.
    val region: StateFlow<FrameRegion?>
        field = MutableStateFlow<FrameRegion?>(null)

    // Why the stream stopped against the runtime's expectation; a loss that the runtime mends by
    // itself has no reason.
    val failure: StateFlow<String?>
        field = MutableStateFlow<String?>(null)
    val undecodable: StateFlow<Boolean>
        field = MutableStateFlow(false)
    val touches: StateFlow<Long>
        field = MutableStateFlow(0L)

    // Text copied on the other side, for this device's clipboard.
    val copies: SharedFlow<String>
        field = MutableSharedFlow<String>(extraBufferCapacity = 8)
    protected val work = CoroutineScope(scope.coroutineContext + pipes.dispatcher)
    private val link = pipes.link(this, key)
    private var joining: Job? = null
    private var active = false
    private var shared = true
    private var closed = false
    private val streaming get() = active && shared && !closed
    private var open = false
    protected var watching = false
        private set
    private var surface: Surface? = null

    // Read on the pipe's thread.
    @Volatile
    private var decoder: VideoDecoder? = null
    private var pixels = IntSize.Zero
    private var density = 1f
    private var size: JsonObject? = null
    private var sent: JsonObject? = null
    private var settling: Job? = null
    private var holding = false

    init {
        link.onNews = ::heard
        pipes.execute { pipes.follow(this) }
    }

    // A message of the stream, on the pipes' thread.
    protected abstract fun take(message: JsonObject)

    // A new pipe is a new seat at the stream, which starts as it is.
    protected open fun opened() = Unit

    // Runs on the pipes' thread, after what is already queued there.
    protected fun execute(job: () -> Unit) = pipes.execute(job)

    // A new surface has no picture yet.
    fun surface(surface: Surface, width: Int, height: Int, density: Float) = pipes.execute {
        this.surface = surface
        picture.value = false
        live.value = false
        undecodable.value = false
        resetDecoder()
        settle(width, height, density)
        restart()
    }

    // A surface must have no producer once its view lets go of it, so this waits until the decoder
    // lets go of it.
    fun detach() {
        val detached = CountDownLatch(1)
        pipes.execute {
            surface = null
            resetDecoder()
            picture.value = false
            live.value = false
            undecodable.value = false
            detached.countDown()
        }
        detached.await(DETACH_MS, TimeUnit.MILLISECONDS)
    }

    // A feed that does not stream keeps no decoder, even on a surface that stays, and the last decoder
    // lets go of the surface before the next one connects to it.
    private fun resetDecoder() {
        decoder?.close()
        decoder = null
        val surface = surface
        if (streaming && surface != null) {
            undecodable.value = false
            decoder = VideoDecoder(
                surface,
                sized = { shown -> pictureSize.value = shown },
                rendered = { shown -> pipes.execute { if (decoder === shown) rendered() } },
                refused = { refused -> pipes.execute { if (decoder === refused) refuse() } },
                lost = { failed -> pipes.execute { if (decoder === failed) restart() } },
                heat = { pipes.heat }
            )
        }
        shows()
    }

    private fun rendered() {
        picture.value = true
        live.value = true
    }

    private fun refuse() {
        decoder?.close()
        decoder = null
        shows()
        live.value = false
        undecodable.value = true
    }

    // The stream shows nothing for a while, so its picture is no longer its own.
    protected fun stale() {
        live.value = false
    }

    // Lets go of the still that the feed keeps for its next view, on the main thread, unless a view
    // shows it.
    internal open fun freeStill() = Unit

    fun resize(width: Int, height: Int, density: Float) = pipes.execute { settle(width, height, density) }

    // The stream's size follows the last active viewer, so debounce resizes and hold them during mouse
    // drags; pending sizes flush after input so clicks hit the layout the user saw. If the pipe opened
    // before layout completed, send the initial watch as soon as we have a size.
    private fun settle(width: Int, height: Int, density: Float) {
        if (width <= 0 || height <= 0) return
        pixels = IntSize(width, height)
        this.density = density
        size = fields(
            "width" to (width / density).toInt().coerceAtLeast(1),
            "height" to (height / density).toInt().coerceAtLeast(1),
            "scale" to pipes.scale(density)
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

    internal fun rescale() = settle(pixels.width, pixels.height, density)

    fun show(on: Boolean) = pipes.execute {
        if (active == on) return@execute
        active = on
        stream()
    }

    // Whether the person at the computer shares what the feed shows; the feed keeps no pipe open while
    // they do not.
    fun share(on: Boolean) = pipes.execute {
        if (shared == on) return@execute
        shared = on
        stream()
    }

    // The runtime lets go of the pipe before its decoder closes, so that no frame's call still runs on it.
    private fun stream() {
        if (streaming) {
            resetDecoder()
            join()
        } else {
            open = false
            watching = false
            joining?.cancel()
            link.detach()
            resetDecoder()
            live.value = false
        }
    }

    // Opens a new pipe at once rather than after the runtime's next try, which waits longer after each
    // failure.
    fun retry() = pipes.execute {
        failure.value = null
        live.value = false
        open = false
        watching = false
        link.detach()
        if (decoder == null) resetDecoder()
        join()
    }

    internal fun chosen() {
        if (watching) watch()
    }

    // Whether the stream plays this viewer's input.
    protected open val takesInput: Boolean get() = true

    fun input(value: JsonObject) = pipes.execute {
        if (!takesInput) return@execute
        pipes.wake(this)
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
        if (!streaming) return
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
        if (!streaming) return
        when (news) {
            DocumentLink.News.Open -> {
                failure.value = null
                open = true
                opened()
                watch()
            }

            is DocumentLink.News.Lost -> {
                open = false
                watching = false
                failure.value = news.reason
                live.value = false
            }

            DocumentLink.News.Closed -> {
                open = false
                watching = false
                live.value = false
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

    // On the pipe's thread: hands the frame to the decoder, and returns false when the decoder cannot go on. The bytes
    // are valid only during the call.
    internal fun frame(codec: VideoCodec, key: Boolean, shows: FrameRegion?, bytes: ByteBuffer): Boolean {
        val decoder = decoder
        val took = decoder?.frame(bytes, key, codec) ?: true
        execute {
            pipes.wake(this)
            if (took && decoder != null && this.decoder === decoder) {
                region.value = shows
                live.value = true
            }
        }
        return took
    }

    // On the pipe's thread.
    internal fun message(json: ByteArray) {
        val message = runCatching { RuntimeJson.parseToJsonElement(json.decodeToString()) as? JsonObject }.getOrNull()
            ?: return
        execute {
            if (!streaming) return@execute
            message.string("copied")?.let(copies::tryEmit)
            take(message)
        }
    }

    // On the pipe's thread: whether the picture is not the stream's newest any more. A rest ends with the next frame, or
    // with false when the picture is the newest again.
    internal fun stale(stale: Boolean) = execute {
        if (!streaming) return@execute
        if (stale) {
            stale()
        } else if (picture.value && decoder != null) {
            live.value = true
        }
    }

    // The decoder starts over, so the runtime waits for a keyframe and asks for one.
    private fun restart() {
        if (decoder != null) link.pipe?.let(pipes::restart)
    }

    protected fun post(value: JsonObject) {
        if (active) link.send(byteArrayOf(0) + value.toString().encodeToByteArray())
    }

    override fun close() = pipes.execute {
        closed = true
        active = false
        open = false
        watching = false
        live.value = false
        pipes.unfollow(this)
        joining?.cancel()
        link.close()
        decoder?.close()
        decoder = null
        shows()
    }

    private fun shows() = pipes.shows(this, active && decoder != null)

    private companion object {
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

// The pipe's thread hands frames to the decoder while MediaCodec calls back on the decoder's own thread and the feed
// closes it on the pipes' thread, so the codec's state is under one lock.
internal class VideoDecoder(
    private val surface: Surface,
    private val sized: (IntSize) -> Unit,
    private val rendered: (VideoDecoder) -> Unit,
    private val refused: (VideoDecoder) -> Unit,
    private val lost: (VideoDecoder) -> Unit,
    private val heat: () -> Int = { PowerManager.THERMAL_STATUS_NONE },
    private val decoders: (VideoCodec, IntSize) -> List<String> = ::deviceDecoders,
    private val fullSpeed: (String) -> Map<String, Int> = { fullSpeedRequest(it) }
) : AutoCloseable {
    private val thread = HandlerThread("Kodosi video decoder").apply { start() }
    private val handler = Handler(thread.looper)
    private val lock = ReentrantLock()

    // An input buffer came, or the codec changed.
    private val changed = lock.newCondition()
    private var codec: MediaCodec? = null
    private var name: String? = null
    private var configured: Stream? = null
    private var closed = false
    private var waiting = true
    private var showed = false

    // The codec was configured with the full-speed request.
    private var askedFullSpeed = false

    // A frame's call waits for an input buffer, so it hears of a failure from the codec that changed.
    private var calling = false

    // The keyframe that the codec started with, until it shows a picture.
    private var start: Keyframe? = null

    // The keyframe that the next codec tries, until an input buffer takes it.
    private var again: Keyframe? = null
    private val failures = mutableMapOf<String, Int>()
    private val buffers = ArrayDeque<Int>()

    // On the pipe's thread: copies the bytes into an input buffer, waiting up to WAIT for one, and returns false when
    // the decoder cannot go on. A closed decoder takes every frame.
    fun frame(bytes: ByteBuffer, key: Boolean, type: VideoCodec): Boolean = lock.withLock {
        if (closed) return true
        if (bytes.remaining() > MAX_FRAME) return false
        calling = true
        try {
            if (key) {
                again = null
                val stream = type.pictureSize(bytes)?.let { Stream(type, it) }
                if (stream != null && stream != configured) {
                    reset()
                    start = Keyframe(bytes.copied(), stream)
                    create(stream)
                }
            } else if (waiting) {
                return false
            }
            val current = checkNotNull(codec)
            waiting = false
            var left = WAIT.inWholeNanoseconds
            while (buffers.isEmpty()) {
                if (left <= 0) return false
                left = changed.awaitNanos(left)
                if (closed) return true
                if (codec !== current) return false
            }
            queue(current, bytes)
            true
        } catch (_: NoDecoder) {
            refused(this)
            true
        } catch (_: Exception) {
            fail()
            false
        } finally {
            calling = false
        }
    }

    private fun create(stream: Stream) {
        val (type, size) = stream
        val chosen = decoders(type, size).firstOrNull { failures.getOrDefault(it, 0) < TRIES } ?: throw NoDecoder()
        val request = if (chosen in refusingFullSpeed || heat() >= PowerManager.THERMAL_STATUS_MODERATE) {
            emptyMap()
        } else {
            fullSpeed(chosen)
        }
        name = chosen
        showed = false
        configured = stream
        val next = MediaCodec.createByCodecName(chosen)
        codec = next
        next.setCallback(
            object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(codec: MediaCodec, index: Int) {
                    lock.withLock {
                        if (closed || this@VideoDecoder.codec !== codec) return
                        buffers.addLast(index)
                        val keyframe = again
                        if (keyframe != null) {
                            again = null
                            try {
                                queue(codec, ByteBuffer.wrap(keyframe.bytes))
                            } catch (_: Exception) {
                                failed()
                                return
                            }
                        }
                        changed.signalAll()
                    }
                }

                // A timestamp of now shows the picture at the next refresh of the display.
                override fun onOutputBufferAvailable(codec: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    lock.withLock {
                        if (closed || this@VideoDecoder.codec !== codec) return
                        val shows = info.size > 0
                        runCatching {
                            if (shows) {
                                codec.releaseOutputBuffer(index, System.nanoTime())
                            } else {
                                codec.releaseOutputBuffer(index, false)
                            }
                        }.onSuccess {
                            if (shows && !showed) {
                                showed = true
                                start = null
                                failures.remove(chosen)
                                rendered(this@VideoDecoder)
                            }
                        }
                    }
                }

                override fun onOutputFormatChanged(codec: MediaCodec, format: MediaFormat) {
                    lock.withLock {
                        if (closed || this@VideoDecoder.codec !== codec) return
                        format.shownSize()?.let(sized)
                    }
                }

                override fun onError(codec: MediaCodec, error: MediaCodec.CodecException) {
                    lock.withLock {
                        if (closed || this@VideoDecoder.codec !== codec) return
                        failed()
                    }
                }
            },
            handler
        )
        val video = requireNotNull(next.codecInfo.getCapabilitiesForType(type.mime).videoCapabilities)
        val width = (size.width + video.widthAlignment - 1) / video.widthAlignment * video.widthAlignment
        val height = (size.height + video.heightAlignment - 1) / video.heightAlignment * video.heightAlignment
        askedFullSpeed = request.isNotEmpty()
        next.configure(
            MediaFormat.createVideoFormat(type.mime, width, height).apply {
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, MAX_FRAME)
                setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                setInteger(MediaFormat.KEY_PRIORITY, REALTIME)
                request.forEach { (key, value) -> setInteger(key, value) }
                val supported = next.supportedVendorParameters.toSet()
                VENDOR_LOW_LATENCY.filterKeys { it in supported }.forEach { (key, value) -> setInteger(key, value) }
            },
            surface,
            null,
            0
        )
        next.start()
    }

    private fun queue(codec: MediaCodec, bytes: ByteBuffer) {
        val index = buffers.removeFirst()
        val input = codec.getInputBuffer(index) ?: throw IllegalStateException()
        val size = bytes.remaining()
        if (size > input.capacity()) throw IllegalStateException()
        input.clear()
        input.put(bytes.duplicate())
        codec.queueInputBuffer(index, 0, size, System.nanoTime() / 1000, 0)
    }

    // A failure outside a frame's call, so the feed asks for a keyframe.
    private fun failed() {
        val heard = calling
        fail()
        if (!heard) lost(this)
    }

    // Some decoders (such as the Android emulator's) fail on every frame. A codec that fails before its first picture
    // gets up to TRIES tries with its first keyframe before it is blacklisted and the next one takes over; the frames
    // after that keyframe wait for the next one. A failure with the full-speed request does not count: the codec is
    // tried again at once without it, and is not asked again while the app runs.
    private fun fail() {
        val failed = name.takeUnless { showed }
        val withRequest = askedFullSpeed
        reset()
        if (failed == null) return
        if (withRequest) {
            refusingFullSpeed += failed
        } else {
            failures[failed] = failures.getOrDefault(failed, 0) + 1
        }
        val keyframe = start ?: return
        try {
            create(keyframe.stream)
            again = keyframe
        } catch (_: NoDecoder) {
            refused(this)
        } catch (_: Exception) {
            fail()
        }
    }

    private fun reset() {
        val current = codec
        codec = null
        name = null
        askedFullSpeed = false
        configured = null
        again = null
        runCatching { current?.stop() }
        runCatching { current?.release() }
        buffers.clear()
        waiting = true
        changed.signalAll()
    }

    // Returns once the codec let go of the surface, or after CLOSE_MS.
    override fun close() {
        val released = CountDownLatch(1)
        handler.post {
            lock.withLock {
                closed = true
                reset()
            }
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

    private fun ByteBuffer.copied() = ByteArray(remaining()).also { duplicate().get(it) }

    private class NoDecoder : Exception()

    private data class Stream(val codec: VideoCodec, val size: IntSize)

    private class Keyframe(val bytes: ByteArray, val stream: Stream)

    private companion object {
        const val TRIES = 3
        const val MAX_FRAME = 16 * 1024 * 1024
        const val REALTIME = 0
        val QUALCOMM = listOf("c2.qti.", "OMX.qcom.")
        const val CLOSE_MS = 500L

        // The decoders that failed before their first picture with the full-speed request, while the app runs.
        val refusingFullSpeed: MutableSet<String> = ConcurrentHashMap.newKeySet()

        // Qualcomm's decoders then run at their highest clock, unless the phone is already warm; others may refuse
        // the rate.
        fun fullSpeedRequest(decoder: String): Map<String, Int> = if (QUALCOMM.any(decoder::startsWith)) {
            mapOf(MediaFormat.KEY_OPERATING_RATE to Short.MAX_VALUE.toInt())
        } else {
            emptyMap()
        }

        // About the host's lateness rule: a slower decoder gets fewer frames, a stuck one fails.
        val WAIT = 1.seconds

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
