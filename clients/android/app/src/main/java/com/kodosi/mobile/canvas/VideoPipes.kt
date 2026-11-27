package com.kodosi.mobile.canvas

import android.os.PowerManager
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.VideoRuntime
import com.kodosi.mobile.runtime.VideoSink
import com.kodosi.mobile.terminals.RuntimeSession
import java.nio.ByteBuffer
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeMark
import kotlin.time.TimeSource
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// The video pipes (the shared browser's and the screen's) that this app watches. The runtime hands each watched pipe's
// messages and frames over on a thread of that pipe, so they never wait for the main thread; the feeds' own state stays
// on this executor's single thread.
class VideoPipes(
    private val commands: CommandSink,
    thread: Executor = Executors.newSingleThreadExecutor { Thread(it, "Kodosi video") },
    private val wifi: StreamingWifi? = null,
    power: PowerManager? = null,
    private val time: TimeSource = TimeSource.Monotonic
) : Executor by thread,
    VideoSink {
    val dispatcher: CoroutineDispatcher = thread.asCoroutineDispatcher()

    @Volatile
    var runtime: VideoRuntime? = null

    private val work = CoroutineScope(SupervisorJob() + dispatcher)
    private val links = mutableListOf<DocumentLink>()
    private val feeds = mutableListOf<VideoFeed>()
    private val watched = ConcurrentHashMap<String, VideoFeed>()

    // Each terminal's screen and browser feed lasts while the terminal's canvas holds it, so that its
    // still, zoom and sources wait for the next view. On the main thread.
    private val kept = mutableMapOf<Pair<String, String>, VideoFeed>()
    private val showing = mutableSetOf<VideoFeed>()
    private val chosen = mutableSetOf<String>()
    private lateinit var lastWake: TimeMark
    private var awake: Job? = null
    private var hot = false

    // Android's thermal status, which rises from THERMAL_STATUS_NONE as the phone heats. Decoders read
    // it on their own threads.
    @Volatile
    internal var heat = PowerManager.THERMAL_STATUS_NONE
        private set

    init {
        power?.addThermalStatusListener { status -> execute { heated(status) } }
    }

    override fun frame(pipe: String, codec: Int, key: Boolean, region: FloatArray?, bytes: ByteBuffer): Boolean {
        val feed = watched[pipe] ?: return true
        val type = VideoCodec.of(codec) ?: return false
        return feed.frame(type, key, region?.let { FrameRegion(it[0], it[1], it[2], it[3]) }, bytes)
    }

    override fun message(pipe: String, json: ByteArray) {
        watched[pipe]?.message(json)
    }

    override fun stale(pipe: String, stale: Boolean) {
        watched[pipe]?.stale(stale)
    }

    // Called on this executor's thread whenever a feed starts or stops decoding onto a surface.
    internal fun shows(feed: VideoFeed, on: Boolean) {
        if (on) showing.add(feed) else showing.remove(feed)
        if (showing.isEmpty()) rest()
    }

    // Called on this executor's thread for each video frame that reaches a feed and each input sent
    // through it. Wi-Fi stays out of its power saving only while pictures arrive or the person acts, so
    // that a still page, an asleep browser or a pipe that reconnects lets it save power, while the
    // answer to a first tap after a quiet spell is not held back.
    internal fun wake(feed: VideoFeed) {
        if (feed !in showing) return
        lastWake = time.markNow()
        if (awake != null) return
        wifi?.keep(true)
        awake = work.launch {
            while (lastWake.elapsedNow() < QUIET) delay(QUIET - lastWake.elapsedNow())
            awake = null
            wifi?.keep(false)
        }
    }

    private fun rest() {
        val check = awake ?: return
        check.cancel()
        awake = null
        wifi?.keep(false)
    }

    // A hot phone asks the host for pictures of half its density.
    internal fun scale(density: Float) = if (hot) (density / 2).coerceAtLeast(1f) else density

    // A phone turns hot at severe but cools only below moderate: one that hovers around severe would
    // otherwise resize every stream, and pay for a keyframe, at each change.
    private fun heated(status: Int) {
        heat = status
        val hotNow = status >= PowerManager.THERMAL_STATUS_SEVERE ||
            (hot && status >= PowerManager.THERMAL_STATUS_MODERATE)
        if (hotNow == hot) return
        hot = hotNow
        feeds.forEach(VideoFeed::rescale)
    }

    // Methods on the returned link must only be called on this executor's thread.
    internal fun link(feed: VideoFeed, key: String): DocumentLink {
        val watch = object : DocumentLink.Watch {
            override fun watch(pipe: String) {
                watched[pipe] = feed
                runtime?.watchVideo(pipe)
            }

            override fun unwatch(pipe: String) {
                watched.remove(pipe)
                runtime?.unwatchVideo(pipe)
            }
        }
        return DocumentLink(commands, feed.session, key, watch) { links.remove(it) }
            .also { link -> execute { links.add(link) } }
    }

    // The decoder of the pipe started over, so the runtime waits for a keyframe and asks for one.
    internal fun restart(pipe: String) {
        runtime?.restartVideo(pipe)
    }

    // The opens, losses and closes of the pipes, which come on the main thread.
    fun receive(event: RuntimeEvent) {
        if (event.type !in LIFECYCLE) return
        execute { links.toList().forEach { it.receive(event) } }
    }

    fun renewPipes() = execute { links.toList().forEach(DocumentLink::renewAfterLoss) }

    // Mark that the user explicitly opened the browser or the screen here so its size follows this
    // device (immediately if watching, or on the next watch). Returning from background or
    // reconnecting without choose() leaves the size to whichever device was active last.
    fun choose(session: String) = execute {
        chosen += session
        feeds.filter { it.session == session }.forEach(VideoFeed::chosen)
    }

    // Clear any pending claim when leaving the browser or the screen before watch is sent.
    fun forget(session: String) = execute { chosen -= session }

    internal fun takeChoice(session: String) = chosen.remove(session)

    internal fun follow(feed: VideoFeed) {
        feeds += feed
    }

    internal fun unfollow(feed: VideoFeed) {
        feeds -= feed
    }

    fun screen(session: String): ScreenFeed =
        kept.getOrPut(session to SCREEN) { ScreenFeed(this, session, work, VideoCodec.decodable) } as ScreenFeed

    fun browser(session: String): BrowserFeed =
        kept.getOrPut(session to BROWSER) { BrowserFeed(this, session, work, VideoCodec.decodable) } as BrowserFeed

    // Drops the feeds whose terminal is gone or whose canvas no longer holds them.
    fun keep(sessions: List<RuntimeSession>) {
        val held = sessions.flatMap { session ->
            listOfNotNull(
                (session.id to SCREEN).takeIf { session.canvas?.screen != null },
                (session.id to BROWSER).takeIf { session.canvas?.browser != null }
            )
        }.toSet()
        kept.entries.removeAll { (part, feed) -> (part !in held).also { gone -> if (gone) drop(feed) } }
    }

    // Lets go of the stills that no view shows, when the system runs short of memory.
    fun trim() = kept.values.forEach(VideoFeed::freeStill)

    private fun drop(feed: VideoFeed) {
        feed.close()
        feed.freeStill()
    }

    fun reset() {
        kept.values.forEach(::drop)
        kept.clear()
        execute {
            links.toList().forEach(DocumentLink::close)
            chosen.clear()
        }
    }

    private companion object {
        val QUIET = 5.seconds
        const val SCREEN = "screen"
        const val BROWSER = "browser"
        val LIFECYCLE = setOf("document.opened", "document.lost", "document.closed")
    }
}
