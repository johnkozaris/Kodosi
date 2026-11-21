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

/**
 * The shared browser's pipes run on a thread of their own, from the runtime's events of each pipe
 * to the parts of each frame, its acknowledgment and its way to the decoder. The host reads a late
 * acknowledgment as a slow link and lowers the bitrate, so no frame waits for the main thread, and
 * only what a view shows goes there.
 */
class BrowserPipes(
    private val commands: CommandSink,
    thread: Executor = Executors.newSingleThreadExecutor { Thread(it, "Kodosi browser") }
) : Executor by thread {
    val dispatcher: CoroutineDispatcher = thread.asCoroutineDispatcher()

    private val links = mutableListOf<DocumentLink>()

    /** The key and the id of each open pipe, as the runtime writes them in the events of the pipe. */
    @Volatile
    private var claimed = emptyList<ByteArray>()

    /**
     * The runtime asks on its own threads for each event. Only the events of a pipe have a "pipe" key,
     * and a quote in a text value has an escape, so no other event has these bytes.
     */
    fun claims(json: ByteArray): Boolean = claimed.any(json::holds)

    /** The link must be used on this thread only. */
    fun link(session: String): DocumentLink = DocumentLink(commands, session, "browser", ::claim) {
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
                    faults.browserBehind(arrival.events, arrival.bytes)
                    links.toList().forEach(DocumentLink::renewAfterLoss)
                }
            }
            // The acts of a view wait here for the event at hand, not for each event in the inbox.
            yield()
        }
    }

    fun receive(event: RuntimeEvent) = links.toList().forEach { it.receive(event) }

    fun reset() = execute { links.toList().forEach(DocumentLink::close) }

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
