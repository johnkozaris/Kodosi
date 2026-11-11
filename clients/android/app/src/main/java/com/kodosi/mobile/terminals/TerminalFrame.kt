package com.kodosi.mobile.terminals

import java.nio.ByteBuffer

/** The visible screen of a terminal, cell by cell, as the kernel wrote it. */
class TerminalFrame(
    val cols: Int,
    val rows: Int,
    val cursor: Cursor,
    val foreground: Int,
    val background: Int,
    val cursorColor: Int,
    val scroll: Scroll,
    val mouseTracking: Boolean,
    private val foregrounds: IntArray,
    private val backgrounds: IntArray,
    private val styles: IntArray,
    private val texts: Array<String?>
) {
    enum class CursorStyle { BAR, BLOCK, UNDERLINE, HOLLOW }

    data class Cursor(
        val column: Int,
        val row: Int,
        val visible: Boolean,
        val style: CursorStyle,
        val wideTail: Boolean
    )

    data class Scroll(val total: Int, val offset: Int, val length: Int)

    object Style {
        const val BOLD = 1 shl 0
        const val ITALIC = 1 shl 1
        const val UNDERLINE = 1 shl 2
        const val STRIKETHROUGH = 1 shl 3
        const val OVERLINE = 1 shl 4
        const val SELECTED = 1 shl 5
        const val HYPERLINK = 1 shl 6
    }

    private fun index(column: Int, row: Int) = row * cols + column

    fun foreground(column: Int, row: Int): Int = foregrounds[index(column, row)]

    /** The background of a cell, or zero for the terminal's own. */
    fun background(column: Int, row: Int): Int = backgrounds[index(column, row)]

    fun style(column: Int, row: Int): Int = styles[index(column, row)] and 0xFFFF

    /** Two for a wide character, one for a narrow one, zero for the cell after a wide one. */
    fun width(column: Int, row: Int): Int = styles[index(column, row)] ushr 16

    fun text(column: Int, row: Int): String? = texts[index(column, row)]

    companion object {
        private val ascii = Array(128) { it.toChar().toString() }

        fun read(buffer: ByteBuffer): TerminalFrame {
            val cols = buffer.getShort(0).toInt() and 0xFFFF
            val rows = buffer.getShort(2).toInt() and 0xFFFF
            val cursor = Cursor(
                column = buffer.getShort(4).toInt() and 0xFFFF,
                row = buffer.getShort(6).toInt() and 0xFFFF,
                visible = buffer.get(8).toInt() != 0,
                style = CursorStyle.entries.getOrElse(buffer.get(9).toInt()) { CursorStyle.BLOCK },
                wideTail = buffer.get(11).toInt() != 0
            )
            val count = cols * rows
            val foregrounds = IntArray(count)
            val backgrounds = IntArray(count)
            val styles = IntArray(count)
            val texts = arrayOfNulls<String>(count)
            var at = HEADER_BYTES
            val bytes = ByteArray(255)
            for (cell in 0 until count) {
                foregrounds[cell] = buffer.getInt(at)
                backgrounds[cell] = buffer.getInt(at + 4)
                val width = buffer.get(at + 10).toInt() and 0xFF
                styles[cell] = (buffer.getShort(at + 8).toInt() and 0xFFFF) or (width shl 16)
                val length = buffer.get(at + 11).toInt() and 0xFF
                at += CELL_BYTES
                if (length == 1 && buffer.get(at) >= 0) {
                    texts[cell] = ascii[buffer.get(at).toInt()]
                } else if (length > 0) {
                    buffer.position(at)
                    buffer.get(bytes, 0, length)
                    texts[cell] = bytes.decodeToString(0, length)
                }
                at += length
            }
            return TerminalFrame(
                cols = cols,
                rows = rows,
                cursor = cursor,
                foreground = buffer.getInt(12),
                background = buffer.getInt(16),
                cursorColor = buffer.getInt(20),
                scroll = Scroll(buffer.getInt(24), buffer.getInt(28), buffer.getInt(32)),
                mouseTracking = buffer.get(10).toInt() != 0,
                foregrounds = foregrounds,
                backgrounds = backgrounds,
                styles = styles,
                texts = texts
            )
        }

        private const val HEADER_BYTES = 36
        private const val CELL_BYTES = 12
    }
}
