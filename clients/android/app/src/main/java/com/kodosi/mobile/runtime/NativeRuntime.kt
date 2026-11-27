package com.kodosi.mobile.runtime

import android.content.Context
import java.nio.ByteBuffer

/** The runtime calls these on its own threads. Each call must return quickly, except [onVideoFrame]. */
interface RuntimeCallbacks {
    fun onEvent(json: ByteArray)

    /** The bytes are valid only during the call. */
    fun onTerminalData(sessionId: String, subscriptionId: String, generation: Long, sequence: Long, bytes: ByteBuffer)

    fun onTerminalControl(sessionId: String, subscriptionId: String, generation: Long, json: ByteArray)

    fun onTerminalConnectResult(sessionId: String, subscriptionId: String, generation: Long, result: Int)

    /** The snapshot is valid only during the call. */
    fun onTerminalCheckpoint(
        sessionId: String,
        subscriptionId: String,
        generation: Long,
        nextSequence: Long,
        rows: Int,
        cols: Int,
        console: Boolean,
        snapshot: ByteBuffer
    ): Int

    /**
     * On the pipe's own thread, in pipe order with its messages; it may wait about a second for a slow decoder, never
     * for the main thread. [region] is null when the frame shows the whole source, and the bytes are valid only during
     * the call. Returns [Ffi.VIDEO_TAKEN], also when the pipe has no decoder now, or [Ffi.VIDEO_FAILED] when the decoder
     * cannot go on.
     */
    fun onVideoFrame(pipe: String, codec: Int, kind: Int, region: FloatArray?, bytes: ByteBuffer): Int

    fun onVideoMessage(pipe: String, json: ByteArray)

    fun onVideoRepair(pipe: String)
}

internal object NativeRuntime {
    init {
        System.loadLibrary("kodosi_android")
    }

    @JvmStatic external fun abiVersion(): Int

    @JvmStatic external fun protocolVersion(): Int

    @JvmStatic external fun setEnvironment(name: String, value: ByteArray): Boolean

    // Passes the Android context so the runtime can verify TLS certificates.
    @JvmStatic external fun prepare(context: Context): Int

    @JvmStatic external fun setLanguage(language: String)

    @JvmStatic external fun start(callbacks: RuntimeCallbacks): Long

    @JvmStatic external fun stop(handle: Long)

    @JvmStatic external fun lastStartFailure(): String?

    @JvmStatic external fun sendCommand(handle: Long, json: ByteArray): Int

    @JvmStatic external fun log(level: Int, line: ByteArray): Int

    /** The runtime's decision on a link, as JSON. [computer] is empty for a link of this device. */
    @JvmStatic external fun linkDecision(link: ByteArray, computer: ByteArray): ByteArray?

    @JvmStatic
    external fun terminalConnect(handle: Long, sessionId: String, subscriptionId: String, generation: Long): Int

    @JvmStatic
    external fun terminalDisconnect(handle: Long, sessionId: String, subscriptionId: String, generation: Long): Int

    @JvmStatic external fun terminalInput(
        handle: Long,
        sessionId: String,
        incarnationId: String,
        subscriptionId: String,
        generation: Long,
        bytes: ByteArray
    ): Int

    @JvmStatic external fun videoWatch(handle: Long, pipe: String): Int

    @JvmStatic external fun videoRestart(handle: Long, pipe: String): Int

    // Returns once no video callback of the pipe runs and none will come.
    @JvmStatic external fun videoUnwatch(handle: Long, pipe: String): Int
}

object Ffi {
    const val ABI_VERSION = 12
    const val PROTOCOL_VERSION = 118
    const val OK = 0
    const val RUNTIME_STOPPED = 5
    const val BUSY = 6
    const val SESSION_NOT_FOUND = 7
    const val STALE_SUBSCRIPTION = 8
    const val CHECKPOINT_REJECTED = 9
    const val LOG_WARNING = 1
    const val LOG_ERROR = 2
    const val VIDEO_TAKEN = 0
    const val VIDEO_FAILED = 1
    const val VIDEO_KEY = 1
}
