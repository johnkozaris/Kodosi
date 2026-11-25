package com.kodosi.mobile.runtime

import kotlinx.coroutines.channels.Channel

// Bounded event queue: on overflow, drops buffered events and yields Arrival.Behind before newer
// events so the consumer resyncs. MAX_BYTES fits a full max-size canvas transfer/frame in flight.
class RuntimeEventInbox(private val maxBytes: Long = MAX_BYTES, private val maxCount: Int = MAX_COUNT) {
    sealed interface Arrival {
        class Event(val event: RuntimeEvent) : Arrival

        data class Unreadable(val reason: String) : Arrival

        data class Behind(val events: Int, val bytes: Long) : Arrival
    }

    private val queue = ArrayDeque<ByteArray>()
    private var queuedBytes = 0L
    private var dropped: Arrival.Behind? = null
    private var closed = false
    private val arrived = Channel<Unit>(Channel.CONFLATED)

    // Called on runtime callback threads; never blocks on the consumer.
    fun push(json: ByteArray) {
        synchronized(queue) {
            if (closed) return
            if (queue.size < maxCount && queuedBytes + json.size <= maxBytes) {
                queue.addLast(json)
                queuedBytes += json.size
            } else {
                val earlier = dropped
                dropped = Arrival.Behind(
                    (earlier?.events ?: 0) + queue.size + 1,
                    (earlier?.bytes ?: 0) + queuedBytes + json.size
                )
                queue.clear()
                queuedBytes = 0
            }
        }
        arrived.trySend(Unit)
    }

    suspend fun next(): Arrival? {
        while (true) {
            val json = synchronized(queue) {
                dropped?.let {
                    dropped = null
                    return it
                }
                if (closed) return null
                queue.removeFirstOrNull()?.also { queuedBytes -= it.size }
            }
            if (json != null) return read(json)
            arrived.receive()
        }
    }

    fun close() {
        synchronized(queue) {
            closed = true
            dropped = null
            queue.clear()
            queuedBytes = 0
        }
        arrived.trySend(Unit)
    }

    private fun read(json: ByteArray): Arrival = try {
        Arrival.Event(RuntimeEvent.parse(json))
    } catch (failure: IllegalArgumentException) {
        Arrival.Unreadable(failure.message.orEmpty())
    }

    private companion object {
        const val MAX_BYTES = 64L * 1024 * 1024
        const val MAX_COUNT = 4096
    }
}
