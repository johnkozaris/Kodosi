package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeEventFaults
import com.kodosi.mobile.runtime.RuntimeEventInbox
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.yield

// Video pipes (the shared browser's and the screen's) run on a dedicated single thread so frame
// reassembly, acks, and decoder handoff never block on the main thread (late acks cause the host to
// lower bitrate).
class VideoPipes(
    private val commands: CommandSink,
    thread: Executor = Executors.newSingleThreadExecutor { Thread(it, "Kodosi video") },
    private val wifi: StreamingWifi? = null
) : Executor by thread {
    val dispatcher: CoroutineDispatcher = thread.asCoroutineDispatcher()

    private val links = mutableListOf<DocumentLink>()
    private val feeds = mutableListOf<VideoFeed>()
    private val showing = mutableSetOf<VideoFeed>()
    private val chosen = mutableSetOf<String>()

    @Volatile
    private var claimed = emptyList<ByteArray>()

    // Called on runtime threads to route pipe events off the main queue without parsing JSON.
    // Matching raw `"pipe":"<id>"` bytes is safe because quotes inside JSON string values are
    // escaped and only pipe events have a "pipe" key.
    fun claims(json: ByteArray): Boolean = claimed.any(json::holds)

    // Called on this executor's thread whenever a feed starts or stops decoding onto a surface.
    internal fun shows(feed: VideoFeed, on: Boolean) {
        val before = showing.isNotEmpty()
        if (on) showing.add(feed) else showing.remove(feed)
        if (showing.isNotEmpty() != before) wifi?.keep(showing.isNotEmpty())
    }

    // Methods on the returned link must only be called on this executor's thread.
    fun link(session: String, key: String): DocumentLink = DocumentLink(commands, session, key, ::claim) {
        links.remove(it)
        claim()
    }.also { link -> execute { links.add(link) } }

    suspend fun consume(inbox: RuntimeEventInbox) {
        val faults = RuntimeEventFaults()
        while (true) {
            when (val arrival = inbox.next() ?: return) {
                is RuntimeEventInbox.Arrival.Event -> receive(arrival.event)

                is RuntimeEventInbox.Arrival.Unreadable -> faults.unreadable(null, arrival.reason)

                is RuntimeEventInbox.Arrival.Behind -> {
                    faults.videoBehind(arrival.events, arrival.bytes)
                    links.toList().forEach(DocumentLink::renewAfterLoss)
                }
            }
            // Yield between events so UI actions queued on this executor are not starved.
            yield()
        }
    }

    fun receive(event: RuntimeEvent) = links.toList().forEach { it.receive(event) }

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

    fun reset() = execute {
        links.toList().forEach(DocumentLink::close)
        chosen.clear()
    }

    private fun claim() {
        claimed = links.mapNotNull { link -> link.pipe?.let { "\"pipe\":\"$it\"".encodeToByteArray() } }
    }
}

private fun ByteArray.holds(part: ByteArray): Boolean {
    var start = 0
    while (start <= size - part.size) {
        var at = 0
        while (at < part.size && this[start + at] == part[at]) at++
        if (at == part.size) return true
        start++
    }
    return false
}
