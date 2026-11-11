package com.kodosi.mobile.terminals

import android.graphics.Bitmap
import android.graphics.Rect
import android.os.SystemClock
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.ceil

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
    private val images = LinkedHashMap<Long, Bitmap>()
    private var imageScale = 1.0
    var animationDelay: Long = -1
        private set

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
        animationDelay = nativeTick(handle, SystemClock.uptimeMillis())
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

    fun images(): List<TerminalImage> = synchronized(this) {
        if (handle == 0L) return emptyList()
        val placements = nativeImages(handle) ?: return emptyList()
        val unique = placements.indices.step(16).associateBy { placements[it + 1] }
        val visible = unique.keys
        images.keys.retainAll(visible)
        val pixels = unique.values.sumOf { placements[it + 14].toDouble() * placements[it + 15] }
        val scale = minOf(1.0, MAX_IMAGE_PIXELS / pixels.coerceAtLeast(1.0))
        if (imageScale != scale) images.clear()
        imageScale = scale
        val result = mutableListOf<TerminalImage>()
        for (at in placements.indices step 16) {
            val generation = placements[at + 1]
            val maximum = (placements[at + 14].toDouble() * placements[at + 15] * scale).toLong().coerceAtLeast(1)
            val bitmap = images[generation] ?: nativeImage(handle, placements[at], generation, maximum)?.also {
                images[generation] = it
            } ?: continue
            val sourceWidth = placements[at + 14].toFloat()
            val sourceHeight = placements[at + 15].toFloat()
            if (sourceWidth <= 0 || sourceHeight <= 0) continue
            val scaleX = bitmap.width / sourceWidth
            val scaleY = bitmap.height / sourceHeight
            val left = (placements[at + 10] * scaleX).toInt().coerceIn(0, bitmap.width - 1)
            val top = (placements[at + 11] * scaleY).toInt().coerceIn(0, bitmap.height - 1)
            result += TerminalImage(
                bitmap, placements[at + 2].toInt(), placements[at + 3].toInt(),
                placements[at + 4].toInt(), placements[at + 5].toInt(),
                placements[at + 6].toInt(), placements[at + 7].toInt(),
                placements[at + 8].toInt(), placements[at + 9].toInt(),
                Rect(
                    left,
                    top,
                    ceil((placements[at + 10] + placements[at + 12]) * scaleX).toInt().coerceIn(left + 1, bitmap.width),
                    ceil((placements[at + 11] + placements[at + 13]) * scaleY).toInt().coerceIn(top + 1, bitmap.height)
                )
            )
        }
        result.sortedWith(compareBy(TerminalImage::layer, TerminalImage::z))
    }

    fun clipboard(): ByteArray? = synchronized(this) {
        if (handle == 0L) null else nativeClipboard(handle)
    }

    fun encodeKey(key: Key, modifiers: Int = 0, text: String? = null): ByteArray? = synchronized(this) {
        if (handle == 0L) null else nativeEncodeKey(handle, key.ordinal, modifiers, text)
    }

    fun encodePaste(text: String): ByteArray? = synchronized(this) {
        if (handle == 0L) null else nativeEncodePaste(handle, text.encodeToByteArray())
    }

    fun link(column: Int, row: Int): String? = synchronized(this) {
        if (handle == 0L) null else nativeLink(handle, column, row)?.decodeToString()
    }

    fun select(column: Int, row: Int, word: Boolean) = synchronized(this) {
        if (handle != 0L) nativeSelect(handle, column, row, word)
    }

    fun clearSelection() = select(-2, -2, word = false)

    fun selected(): String? = synchronized(this) {
        if (handle == 0L) null else nativeSelected(handle)?.decodeToString()
    }

    fun mouse(
        action: Int,
        x: Float,
        y: Float,
        width: Int,
        height: Int,
        cellWidth: Int,
        cellHeight: Int,
        button: Int = 1,
        modifiers: Int = 0
    ): ByteArray? = synchronized(this) {
        if (handle ==
            0L
        ) {
            null
        } else {
            nativeMouse(handle, action, x, y, width, height, cellWidth, cellHeight, button, modifiers)
        }
    }

    /** Moves the view through the history; zero returns to the bottom. Tells whether it moved. */
    fun scroll(rows: Int): Boolean = synchronized(this) { handle != 0L && nativeScroll(handle, rows) }

    override fun close() = synchronized(this) {
        if (handle != 0L) nativeDestroy(handle)
        handle = 0
        images.clear()
    }

    companion object {
        const val OK = 0
        private const val CLOSED = 1

        init {
            System.loadLibrary("kodosi_android")
        }

        private const val INITIAL_FRAME_BYTES = 256 * 1024
        private const val FAILURE_CODES = 3
        private const val MAX_IMAGE_PIXELS = 16 * 1024 * 1024

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

        @JvmStatic private external fun nativeLink(handle: Long, column: Int, row: Int): ByteArray?

        @JvmStatic private external fun nativeSelect(handle: Long, column: Int, row: Int, word: Boolean)

        @JvmStatic private external fun nativeSelected(handle: Long): ByteArray?

        @JvmStatic private external fun nativeMouse(
            handle: Long,
            action: Int,
            x: Float,
            y: Float,
            width: Int,
            height: Int,
            cellWidth: Int,
            cellHeight: Int,
            button: Int,
            modifiers: Int
        ): ByteArray?

        @JvmStatic
        fun decodePng(bytes: ByteArray): Bitmap? = TerminalImage.decodePng(bytes)

        @JvmStatic private external fun nativeImages(handle: Long): LongArray?

        @JvmStatic
        private external fun nativeImage(handle: Long, id: Long, generation: Long, maximumPixels: Long): Bitmap?

        @JvmStatic private external fun nativeTick(handle: Long, now: Long): Long

        @JvmStatic private external fun nativeClipboard(handle: Long): ByteArray?
    }
}
