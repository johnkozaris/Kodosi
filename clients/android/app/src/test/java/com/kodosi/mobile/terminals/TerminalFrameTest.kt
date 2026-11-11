package com.kodosi.mobile.terminals

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TerminalFrameTest {
    @Test
    fun joinsSoftWrappedLinksAndDoesNotOpenClippedOnes() {
        val complete = frame(listOf("See https://example.", "org/a(b) here"), byteArrayOf(1, 2))
        assertEquals("https://example.org/a(b)", complete.link(18, 0))
        assertEquals("https://example.org/a(b)", complete.link(2, 1))
        assertNull(complete.link(18, 1))

        val clippedStart = frame(listOf("https://example.org/", "b end"), byteArrayOf(3, 2))
        assertNull(clippedStart.link(2, 0))
        val clippedEnd = frame(listOf("See https://example.", "org/" + "a".repeat(15) + "."), byteArrayOf(1, 3))
        assertNull(clippedEnd.link(8, 0))
    }

    @Test
    fun aWideCharactersTailStillOpensItsLink() {
        val frame = frame(listOf("https://a.org/界 "), widths = mapOf(14 to 2, 15 to 0))
        assertEquals("https://a.org/界", frame.link(15, 0))
    }

    @Test
    fun readsTheNativeRowsBeforeTheirCells() {
        val bytes = ByteBuffer.allocate(36 + 2 + 8 * 13).order(ByteOrder.LITTLE_ENDIAN)
        bytes.putShort(4).putShort(2).position(36)
        repeat(2) { row ->
            bytes.put(if (row == 0) 1 else 2)
            repeat(4) { column ->
                bytes.putInt(-1).putInt(0).putShort(1).put(1).put(1)
                bytes.put(('a'.code + row * 4 + column).toByte())
            }
        }
        val frame = TerminalFrame.read(bytes)
        assertEquals("a", frame.text(0, 0))
        assertEquals("h", frame.text(3, 1))
        assertEquals(1, frame.width(3, 1))
        assertEquals(TerminalFrame.Style.BOLD, frame.style(0, 0))
    }

    private fun frame(
        lines: List<String>,
        flags: ByteArray = ByteArray(lines.size),
        widths: Map<Int, Int> = emptyMap()
    ): TerminalFrame {
        val cols = lines.maxOf { it.length }
        val text = lines.flatMap { it.padEnd(cols).map(Char::toString) }.toTypedArray<String?>()
        return TerminalFrame(
            cols, lines.size, TerminalFrame.Cursor(0, 0, false, TerminalFrame.CursorStyle.BLOCK, false),
            -1, 0, -1, TerminalFrame.Scroll(lines.size, 0, lines.size), false,
            IntArray(text.size), IntArray(text.size),
            IntArray(text.size) { (widths[it] ?: 1) shl 16 }, text, flags
        )
    }
}
