package com.kodosi.mobile.terminals

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.Typeface
import android.text.InputType
import android.view.GestureDetector
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.core.content.res.ResourcesCompat
import androidx.core.graphics.withTranslation
import com.kodosi.mobile.R
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/** Draws a terminal from Ghostty's state and types into it with the keyboard. */
@SuppressLint("ViewConstructor")
class TerminalView(context: Context, private val connection: TerminalConnection) : View(context) {
    /** Control for the next key, from the key row. */
    var control = false
        set(value) {
            field = value
            onControlChanged(value)
        }
    var onControlChanged: (Boolean) -> Unit = {}

    private val regular = ResourcesCompat.getFont(context, R.font.jetbrains_mono_regular) ?: Typeface.MONOSPACE
    private val bold = ResourcesCompat.getFont(context, R.font.jetbrains_mono_bold) ?: Typeface.MONOSPACE
    private val italic = ResourcesCompat.getFont(context, R.font.jetbrains_mono_italic) ?: Typeface.MONOSPACE
    private val boldItalic = ResourcesCompat.getFont(context, R.font.jetbrains_mono_bold_italic) ?: Typeface.MONOSPACE
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = regular
        textSize = FONT_SIZE_DP * resources.displayMetrics.density
    }
    private val fill = Paint()
    private val cellWidth = ceil(text.measureText("M")).toInt()
    private val cellHeight = ceil(text.fontMetrics.descent - text.fontMetrics.ascent).toInt()
    private val baseline = -text.fontMetrics.ascent
    private val line = max(1, (resources.displayMetrics.density).toInt())
    private val run = StringBuilder()
    private var scrolled = 0f
    private var accent = 0

    private val gestures = GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(event: MotionEvent) = true

            override fun onSingleTapUp(event: MotionEvent): Boolean {
                showKeyboard()
                return true
            }

            override fun onScroll(first: MotionEvent?, event: MotionEvent, dx: Float, dy: Float): Boolean {
                scrolled += dy
                val rows = (scrolled / cellHeight).toInt()
                if (rows != 0) {
                    scrolled -= rows * cellHeight
                    if (connection.kernel.scroll(rows)) invalidate()
                }
                return true
            }
        }
    )

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        isFocusedByDefault = true
        connection.onChange = { postInvalidateOnAnimation() }
        text.letterSpacing = (cellWidth - text.measureText("M")) / text.textSize
    }

    fun showKeyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java)?.showSoftInput(this, 0)
    }

    /** Presses `key`, with control when the key row holds it for the next key. */
    fun press(key: TerminalKernel.Key) {
        val modifiers = if (control) TerminalKernel.Modifier.CONTROL else 0
        control = false
        send(key, modifiers)
    }

    private fun send(key: TerminalKernel.Key, modifiers: Int) {
        connection.kernel.encodeKey(key, modifiers)?.let(connection::send)
    }

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        if (width <= 0 || height <= 0) return
        val cols = max(1, width / cellWidth)
        val rows = max(MINIMUM_ROWS, height / cellHeight)
        connection.setViewport(Viewport(cols, rows, cellWidth, cellHeight))
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean = gestures.onTouchEvent(event) || super.onTouchEvent(event)

    override fun onFocusChanged(gained: Boolean, direction: Int, previous: Rect?) {
        super.onFocusChanged(gained, direction, previous)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val frame = connection.kernel.frame()
        if (frame == null) {
            canvas.drawColor(TERMINAL_BACKGROUND)
            return
        }
        canvas.drawColor(frame.background)
        canvas.withTranslation(0f, -top(frame).toFloat()) {
            for (row in 0 until frame.rows) {
                drawBackgrounds(this, frame, row)
                drawText(this, frame, row)
            }
            drawCursor(this, frame)
        }
    }

    /** With less room than the terminal's rows, the bottom rows show, or the rows down to the cursor. */
    private fun top(frame: TerminalFrame): Int {
        val hidden = max(0, frame.rows * cellHeight - height)
        val cursorTop = frame.cursor.row * cellHeight
        if (!frame.cursor.visible || cursorTop >= hidden) return hidden
        return min(hidden, max(0, cursorTop + cellHeight - height))
    }

    private fun drawBackgrounds(canvas: Canvas, frame: TerminalFrame, row: Int) {
        val y = row * cellHeight
        var column = 0
        while (column < frame.cols) {
            val color = frame.background(column, row)
            var end = column + 1
            while (end < frame.cols && frame.background(end, row) == color) end++
            if (color != 0) {
                fill.color = color
                canvas.drawRect(
                    (column * cellWidth).toFloat(),
                    y.toFloat(),
                    (end * cellWidth).toFloat(),
                    (y + cellHeight).toFloat(),
                    fill
                )
            }
            column = end
        }
    }

    private fun drawText(canvas: Canvas, frame: TerminalFrame, row: Int) {
        val y = row * cellHeight + baseline
        var column = 0
        while (column < frame.cols) {
            val width = frame.width(column, row)
            val content = frame.text(column, row)
            if (content == null || width == 0) {
                column++
                continue
            }
            val color = frame.foreground(column, row)
            val style = frame.style(column, row)
            run.setLength(0)
            run.append(content)
            var end = column + 1
            if (width == 1) {
                while (end < frame.cols && frame.width(end, row) == 1 && frame.foreground(end, row) == color &&
                    frame.style(end, row) == style
                ) {
                    run.append(frame.text(end, row) ?: " ")
                    end++
                }
            } else {
                end = column + width
            }
            paint(color, style)
            val x = (column * cellWidth).toFloat()
            val span = ((end - column) * cellWidth).toFloat()
            val left = if (width == 1) x else x + max(0f, (span - text.measureText(run, 0, run.length)) / 2)
            canvas.drawText(run, 0, run.length, left, y, text)
            decorate(canvas, style, x, span, row * cellHeight.toFloat())
            column = end
        }
    }

    private fun paint(color: Int, style: Int) {
        val isBold = style and TerminalFrame.Style.BOLD != 0
        val isItalic = style and TerminalFrame.Style.ITALIC != 0
        text.typeface = when {
            isBold && isItalic -> boldItalic
            isBold -> bold
            isItalic -> italic
            else -> regular
        }
        text.color = color
    }

    private fun decorate(canvas: Canvas, style: Int, x: Float, width: Float, top: Float) {
        fill.color = text.color
        if (style and TerminalFrame.Style.UNDERLINE != 0) {
            val y = top + baseline + line * 2
            canvas.drawRect(x, y, x + width, y + line, fill)
        }
        if (style and TerminalFrame.Style.STRIKETHROUGH != 0) {
            val y = top + cellHeight / 2f
            canvas.drawRect(x, y, x + width, y + line, fill)
        }
        if (style and TerminalFrame.Style.OVERLINE != 0) {
            canvas.drawRect(x, top, x + width, top + line, fill)
        }
    }

    private fun drawCursor(canvas: Canvas, frame: TerminalFrame) {
        val cursor = frame.cursor
        if (!cursor.visible || cursor.column >= frame.cols || cursor.row >= frame.rows) return
        val column = if (cursor.wideTail && cursor.column > 0) cursor.column - 1 else cursor.column
        val wide = frame.width(column, cursor.row) == 2
        val left = (column * cellWidth).toFloat()
        val top = (cursor.row * cellHeight).toFloat()
        val right = left + cellWidth * if (wide) 2 else 1
        val bottom = top + cellHeight
        fill.color = frame.cursorColor
        val style = if (hasFocus()) cursor.style else TerminalFrame.CursorStyle.HOLLOW
        when (style) {
            TerminalFrame.CursorStyle.BLOCK -> {
                canvas.drawRect(left, top, right, bottom, fill)
                val content = frame.text(column, cursor.row) ?: return
                paint(frame.background, frame.style(column, cursor.row))
                val glyph = if (wide) left + max(0f, (right - left - text.measureText(content)) / 2) else left
                canvas.drawText(content, glyph, top + baseline, text)
            }

            TerminalFrame.CursorStyle.BAR -> canvas.drawRect(left, top, left + line * 2, bottom, fill)

            TerminalFrame.CursorStyle.UNDERLINE -> canvas.drawRect(left, bottom - line * 2, right, bottom, fill)

            TerminalFrame.CursorStyle.HOLLOW -> {
                fill.style = Paint.Style.STROKE
                fill.strokeWidth = line.toFloat()
                canvas.drawRect(left + line / 2f, top + line / 2f, right - line / 2f, bottom - line / 2f, fill)
                fill.style = Paint.Style.FILL
            }
        }
    }

    override fun onCheckIsTextEditor() = true

    override fun onCreateInputConnection(attributes: EditorInfo): InputConnection {
        attributes.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        attributes.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_ACTION_NONE
        return Typing()
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val modifiers = modifiers(event)
        val key = when (keyCode) {
            KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> TerminalKernel.Key.ENTER
            KeyEvent.KEYCODE_DEL -> TerminalKernel.Key.BACKSPACE
            KeyEvent.KEYCODE_FORWARD_DEL -> TerminalKernel.Key.DELETE
            KeyEvent.KEYCODE_TAB -> TerminalKernel.Key.TAB
            KeyEvent.KEYCODE_ESCAPE -> TerminalKernel.Key.ESCAPE
            KeyEvent.KEYCODE_DPAD_UP -> TerminalKernel.Key.UP
            KeyEvent.KEYCODE_DPAD_DOWN -> TerminalKernel.Key.DOWN
            KeyEvent.KEYCODE_DPAD_LEFT -> TerminalKernel.Key.LEFT
            KeyEvent.KEYCODE_DPAD_RIGHT -> TerminalKernel.Key.RIGHT
            KeyEvent.KEYCODE_MOVE_HOME -> TerminalKernel.Key.HOME
            KeyEvent.KEYCODE_MOVE_END -> TerminalKernel.Key.END
            KeyEvent.KEYCODE_PAGE_UP -> TerminalKernel.Key.PAGE_UP
            KeyEvent.KEYCODE_PAGE_DOWN -> TerminalKernel.Key.PAGE_DOWN
            else -> null
        }
        if (key != null) {
            control = false
            send(key, modifiers)
            return true
        }
        val character = event.getUnicodeChar(event.metaState and MODIFIER_META.inv())
        if (character == 0) return super.onKeyDown(keyCode, event)
        if (character and KeyCharacterMap.COMBINING_ACCENT != 0) {
            accent = character and KeyCharacterMap.COMBINING_ACCENT_MASK
            return true
        }
        val composed = KeyCharacterMap.getDeadChar(accent, character).takeIf { accent != 0 && it != 0 }
        accent = 0
        type(String(Character.toChars(composed ?: character)), modifiers)
        return true
    }

    private fun modifiers(event: KeyEvent): Int {
        var modifiers = 0
        if (event.isShiftPressed) modifiers = modifiers or TerminalKernel.Modifier.SHIFT
        if (event.isCtrlPressed || control) modifiers = modifiers or TerminalKernel.Modifier.CONTROL
        if (event.isAltPressed) modifiers = modifiers or TerminalKernel.Modifier.ALT
        return modifiers
    }

    private fun type(content: String, modifiers: Int = if (control) TerminalKernel.Modifier.CONTROL else 0) {
        if (content.isEmpty()) return
        val modified = modifiers and (TerminalKernel.Modifier.CONTROL or TerminalKernel.Modifier.ALT) != 0
        control = false
        for (piece in content.split('\n').withIndex()) {
            if (piece.index > 0) send(TerminalKernel.Key.ENTER, 0)
            val words = piece.value
            if (words.isEmpty()) continue
            val bytes = if (modified && words.length == 1) {
                connection.kernel.encodeKey(TerminalKernel.Key.CHARACTER, modifiers, words)
            } else {
                words.encodeToByteArray()
            }
            bytes?.let(connection::send)
        }
    }

    private inner class Typing : BaseInputConnection(this@TerminalView, false) {
        private var composing = ""

        override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
            composing = ""
            type(text.toString())
            return true
        }

        override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
            composing = text.toString()
            return true
        }

        override fun finishComposingText(): Boolean {
            val pending = composing
            composing = ""
            type(pending)
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            if (composing.isNotEmpty()) {
                composing = composing.dropLast(beforeLength)
                return true
            }
            repeat(min(beforeLength, MAX_DELETE)) { press(TerminalKernel.Key.BACKSPACE) }
            return true
        }
    }

    companion object {
        const val MINIMUM_ROWS = 10
        private const val MODIFIER_META = KeyEvent.META_CTRL_MASK or KeyEvent.META_ALT_MASK
        private const val FONT_SIZE_DP = 12f
        private const val MAX_DELETE = 64
        private const val TERMINAL_BACKGROUND = 0xFF0D0B09.toInt()
    }
}
