package com.kodosi.mobile.runtime

import android.content.Context
import java.nio.ByteBuffer

/** What the runtime calls on its own threads. Each call must return quickly. */
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
}

internal object NativeRuntime {
    init {
        System.loadLibrary("kodosi_android")
    }

    @JvmStatic external fun abiVersion(): Int

    @JvmStatic external fun protocolVersion(): Int

    @JvmStatic external fun setEnvironment(name: String, value: ByteArray): Boolean

    /** Gives the runtime the app's context, which it needs to check certificates. */
    @JvmStatic external fun prepare(context: Context): Int

    @JvmStatic external fun setLanguage(language: String)

    @JvmStatic external fun start(callbacks: RuntimeCallbacks): Long

    @JvmStatic external fun stop(handle: Long)

    @JvmStatic external fun lastStartFailure(): String?

    @JvmStatic external fun sendCommand(handle: Long, json: ByteArray): Int

    @JvmStatic
    external fun terminalConnect(handle: Long, sessionId: String, subscriptionId: String, generation: Long): Int

    @JvmStatic
    external fun terminalRefresh(handle: Long, sessionId: String, subscriptionId: String, generation: Long): Int

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
}

/** The codes of the C API. */
object Ffi {
    const val ABI_VERSION = 11
    const val PROTOCOL_VERSION = 88
    const val OK = 0
    const val DESER_FAILED = 3
    const val RUNTIME_STOPPED = 5
    const val BUSY = 6
    const val STALE_SUBSCRIPTION = 8
    const val CHECKPOINT_REJECTED = 9
}
