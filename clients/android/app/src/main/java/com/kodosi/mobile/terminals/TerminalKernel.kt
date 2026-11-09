package com.kodosi.mobile.terminals

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Ghostty's terminal state for one view: it takes the runtime's checkpoints and data in order and
 * gives the visible screen to draw. Its calls are safe from any thread, also after it closes.
 */
class TerminalKernel : AutoCloseable {
    enum class Key {
        ENTER,
        BACKSPACE,
        TAB,
        ESCAPE,
        UP,
        DOWN,
        LEFT,
        RIGHT,
        HOME,
        END,
        PAGE_UP,
        PAGE_DOWN,
        DELETE,
        CHARACTER
    }

    object Modifier {
        const val SHIFT = 1
        const val CONTROL = 2
        const val ALT = 4
    }

    private var handle = nativeCreate()
    private var output = ByteBuffer.allocateDirect(INITIAL_FRAME_BYTES).order(ByteOrder.LITTLE_ENDIAN)

    fun install(snapshot: ByteBuffer, nextSequence: Long, rows: Int, cols: Int, console: Boolean): Int =
        synchronized(this) {
            if (handle == 0L) CLOSED else nativeInstall(handle, snapshot, nextSequence, rows, cols, console)
        }

    fun write(bytes: ByteBuffer, sequence: Long): Int = synchronized(this) {
        if (handle == 0L) CLOSED else nativeWrite(handle, bytes, sequence)
    }

    fun resize(atSequence: Long, rows: Int, cols: Int, cellWidth: Int, cellHeight: Int): Int = synchronized(this) {
        if (handle == 0L) CLOSED else nativeResize(handle, atSequence, rows, cols, cellWidth, cellHeight)
    }

    /** The visible screen, or null before the first checkpoint. */
    fun frame(): TerminalFrame? = synchronized(this) {
        if (handle == 0L) return null
        var written = nativeFrame(handle, output)
        if (written < -FAILURE_CODES) {
            output = ByteBuffer.allocateDirect(-written * 2).order(ByteOrder.LITTLE_ENDIAN)
            written = nativeFrame(handle, output)
        }
        if (written <= 0) return null
        output.position(0)
        output.limit(written)
        TerminalFrame.read(output)
    }

    fun encodeKey(key: Key, modifiers: Int = 0, text: String? = null): ByteArray? = synchronized(this) {
        if (handle == 0L) null else nativeEncodeKey(handle, key.ordinal, modifiers, text)
    }

    fun encodePaste(text: String): ByteArray? = synchronized(this) {
        if (handle == 0L) null else nativeEncodePaste(handle, text.encodeToByteArray())
    }

    /** Moves the view through the history; zero returns to the bottom. Tells whether it moved. */
    fun scroll(rows: Int): Boolean = synchronized(this) { handle != 0L && nativeScroll(handle, rows) }

    override fun close() = synchronized(this) {
        if (handle != 0L) nativeDestroy(handle)
        handle = 0
    }

    companion object {
        const val OK = 0
        private const val CLOSED = 1

        init {
            System.loadLibrary("kodosi_android")
        }

        private const val INITIAL_FRAME_BYTES = 256 * 1024
        private const val FAILURE_CODES = 3

        @JvmStatic private external fun nativeCreate(): Long

        @JvmStatic private external fun nativeDestroy(handle: Long)

        @JvmStatic private external fun nativeInstall(
            handle: Long,
            snapshot: ByteBuffer,
            nextSequence: Long,
            rows: Int,
            cols: Int,
            console: Boolean
        ): Int

        @JvmStatic private external fun nativeWrite(handle: Long, data: ByteBuffer, sequence: Long): Int

        @JvmStatic private external fun nativeResize(
            handle: Long,
            atSequence: Long,
            rows: Int,
            cols: Int,
            cellWidth: Int,
            cellHeight: Int
        ): Int

        @JvmStatic private external fun nativeFrame(handle: Long, output: ByteBuffer): Int

        @JvmStatic
        private external fun nativeEncodeKey(handle: Long, key: Int, modifiers: Int, text: String?): ByteArray?

        @JvmStatic private external fun nativeEncodePaste(handle: Long, text: ByteArray): ByteArray?

        @JvmStatic private external fun nativeScroll(handle: Long, rows: Int): Boolean
    }
}
