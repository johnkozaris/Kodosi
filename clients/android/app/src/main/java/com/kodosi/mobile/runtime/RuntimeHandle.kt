package com.kodosi.mobile.runtime

import android.content.Context
import java.nio.ByteBuffer
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.channels.Channel

/** The view of one terminal that the runtime streams to. */
interface TerminalLink {
    val subscriptionId: String
    val generation: Long

    fun onData(sequence: Long, bytes: ByteBuffer)

    fun onControl(json: ByteArray)

    fun onCheckpoint(nextSequence: Long, rows: Int, cols: Int, console: Boolean, snapshot: ByteBuffer): Int

    fun onConnectResult(result: Int)

    companion object {
        private val generations = AtomicLong()

        fun nextGeneration(): Long = generations.incrementAndGet()
    }
}

/** The embedded runtime: its handle, its events and the terminals that it streams. */
class RuntimeHandle : RuntimeCallbacks {
    sealed interface Start {
        data object Started : Start

        data object Incompatible : Start

        data class Failed(val message: String?) : Start
    }

    val events = Channel<ByteArray>(EVENT_CAPACITY)

    @Volatile
    var overflowed = false
        private set

    @Volatile
    private var handle = 0L
    private val links = ConcurrentHashMap<String, TerminalLink>()

    /** Starts the runtime. It blocks, so call it away from the main thread. */
    fun start(context: Context, language: String): Start {
        if (NativeRuntime.abiVersion() != Ffi.ABI_VERSION || NativeRuntime.protocolVersion() != Ffi.PROTOCOL_VERSION) {
            return Start.Incompatible
        }
        if (NativeRuntime.prepare(context.applicationContext) != Ffi.OK) return Start.Failed(null)
        NativeRuntime.setLanguage(language)
        val started = NativeRuntime.start(this)
        if (started == 0L) return Start.Failed(NativeRuntime.lastStartFailure())
        handle = started
        return Start.Started
    }

    /** Stops the runtime and waits for its callbacks to end. */
    fun stop() {
        val current = handle
        handle = 0
        links.clear()
        if (current != 0L) NativeRuntime.stop(current)
        events.close()
    }

    fun send(json: ByteArray): Int? = handle.takeIf { it != 0L }?.let { NativeRuntime.sendCommand(it, json) }

    fun connect(sessionId: String, link: TerminalLink): Int {
        links[sessionId] = link
        val current = handle.takeIf { it != 0L } ?: return Ffi.RUNTIME_STOPPED
        return NativeRuntime.terminalConnect(current, sessionId, link.subscriptionId, link.generation)
    }

    fun refresh(sessionId: String, link: TerminalLink) {
        val current = handle.takeIf { it != 0L } ?: return
        NativeRuntime.terminalRefresh(current, sessionId, link.subscriptionId, link.generation)
    }

    fun disconnect(sessionId: String, link: TerminalLink) {
        links.remove(sessionId, link)
        val current = handle.takeIf { it != 0L } ?: return
        NativeRuntime.terminalDisconnect(current, sessionId, link.subscriptionId, link.generation)
    }

    fun input(sessionId: String, incarnationId: String, link: TerminalLink, bytes: ByteArray): Int {
        val current = handle.takeIf { it != 0L } ?: return Ffi.RUNTIME_STOPPED
        return NativeRuntime.terminalInput(
            current,
            sessionId,
            incarnationId,
            link.subscriptionId,
            link.generation,
            bytes
        )
    }

    private fun link(sessionId: String, subscriptionId: String, generation: Long): TerminalLink? =
        links[sessionId]?.takeIf { it.subscriptionId == subscriptionId && it.generation == generation }

    override fun onEvent(json: ByteArray) {
        if (!events.trySend(json).isSuccess) {
            overflowed = true
            events.close()
        }
    }

    override fun onTerminalData(
        sessionId: String,
        subscriptionId: String,
        generation: Long,
        sequence: Long,
        bytes: ByteBuffer
    ) {
        link(sessionId, subscriptionId, generation)?.onData(sequence, bytes)
    }

    override fun onTerminalControl(sessionId: String, subscriptionId: String, generation: Long, json: ByteArray) {
        link(sessionId, subscriptionId, generation)?.onControl(json)
    }

    override fun onTerminalConnectResult(sessionId: String, subscriptionId: String, generation: Long, result: Int) {
        link(sessionId, subscriptionId, generation)?.onConnectResult(result)
    }

    override fun onTerminalCheckpoint(
        sessionId: String,
        subscriptionId: String,
        generation: Long,
        nextSequence: Long,
        rows: Int,
        cols: Int,
        console: Boolean,
        snapshot: ByteBuffer
    ): Int = link(sessionId, subscriptionId, generation)?.onCheckpoint(nextSequence, rows, cols, console, snapshot)
        ?: Ffi.STALE_SUBSCRIPTION

    companion object {
        private const val EVENT_CAPACITY = 256

        /** Gives the runtime's sentences for people in this language from now on. */
        fun setLanguage(language: String) = NativeRuntime.setLanguage(language)
    }
}
