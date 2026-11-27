package com.kodosi.mobile.runtime

import android.content.Context
import java.nio.ByteBuffer
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

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

interface TerminalRuntime {
    fun connect(sessionId: String, link: TerminalLink): Int

    fun disconnect(sessionId: String, link: TerminalLink)

    fun input(sessionId: String, incarnationId: String, link: TerminalLink, bytes: ByteArray): Int
}

// Where the runtime hands the messages and frames of the video pipes that the app watches, on each pipe's own thread.
interface VideoSink {
    // Returns false when the decoder cannot go on. The bytes are valid only during the call.
    fun frame(pipe: String, codec: Int, key: Boolean, region: FloatArray?, bytes: ByteBuffer): Boolean

    fun message(pipe: String, json: ByteArray)

    fun repair(pipe: String)
}

// The app watches each video pipe (the shared browser's or the screen's) that it opened, until before it closes it.
interface VideoRuntime {
    fun watchVideo(pipe: String)

    // The pipe's decoder started over outside a frame's call, so it needs a keyframe.
    fun restartVideo(pipe: String)

    // Returns once no call of the pipe runs and none will come.
    fun unwatchVideo(pipe: String)
}

class RuntimeHandle(private val video: VideoSink) :
    RuntimeCallbacks,
    TerminalRuntime,
    VideoRuntime {
    sealed interface Start {
        data object Started : Start

        data object Incompatible : Start

        data class Failed(val message: String?) : Start
    }

    val events = RuntimeEventInbox()

    @Volatile
    private var handle = 0L
    private val links = ConcurrentHashMap<String, TerminalLink>()
    private val faults = RuntimeEventFaults()

    // Blocks during native startup; call off the main thread.
    fun start(context: Context, language: String): Start {
        if (NativeRuntime.abiVersion() != Ffi.ABI_VERSION || NativeRuntime.protocolVersion() != Ffi.PROTOCOL_VERSION) {
            return Start.Incompatible
        }
        KeystoreSecrets.initialize(context)
        if (NativeRuntime.prepare(context.applicationContext) != Ffi.OK) return Start.Failed(null)
        NativeRuntime.setLanguage(language)
        val started = NativeRuntime.start(this)
        if (started == 0L) return Start.Failed(NativeRuntime.lastStartFailure())
        handle = started
        return Start.Started
    }

    // Blocks until the runtime's callbacks have ended.
    fun stop() {
        val current = handle
        handle = 0
        links.clear()
        if (current != 0L) NativeRuntime.stop(current)
        events.close()
    }

    fun send(json: ByteArray): Int? = handle.takeIf { it != 0L }?.let { NativeRuntime.sendCommand(it, json) }

    override fun connect(sessionId: String, link: TerminalLink): Int {
        links[sessionId] = link
        val current = handle.takeIf { it != 0L } ?: return Ffi.RUNTIME_STOPPED
        return NativeRuntime.terminalConnect(current, sessionId, link.subscriptionId, link.generation)
    }

    override fun disconnect(sessionId: String, link: TerminalLink) {
        links.remove(sessionId, link)
        val current = handle.takeIf { it != 0L } ?: return
        NativeRuntime.terminalDisconnect(current, sessionId, link.subscriptionId, link.generation)
    }

    override fun input(sessionId: String, incarnationId: String, link: TerminalLink, bytes: ByteArray): Int {
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

    override fun watchVideo(pipe: String) = video(pipe, "watch", NativeRuntime::videoWatch)

    override fun restartVideo(pipe: String) = video(pipe, "restart", NativeRuntime::videoRestart)

    override fun unwatchVideo(pipe: String) = video(pipe, "unwatch", NativeRuntime::videoUnwatch)

    private fun video(pipe: String, act: String, call: (Long, String) -> Int) {
        val current = handle.takeIf { it != 0L } ?: return
        val result = call(current, pipe)
        if (result != Ffi.OK && result != Ffi.SESSION_NOT_FOUND) faults.video(act, result)
    }

    private fun link(sessionId: String, subscriptionId: String, generation: Long): TerminalLink? =
        links[sessionId]?.takeIf { it.subscriptionId == subscriptionId && it.generation == generation }

    override fun onEvent(json: ByteArray) = events.push(json)

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

    override fun onVideoFrame(pipe: String, codec: Int, kind: Int, region: FloatArray?, bytes: ByteBuffer): Int =
        if (video.frame(pipe, codec, kind == Ffi.VIDEO_KEY, region, bytes)) Ffi.VIDEO_TAKEN else Ffi.VIDEO_FAILED

    override fun onVideoMessage(pipe: String, json: ByteArray) = video.message(pipe, json)

    override fun onVideoRepair(pipe: String) = video.repair(pipe)

    companion object {
        fun setLanguage(language: String) = NativeRuntime.setLanguage(language)
    }
}
