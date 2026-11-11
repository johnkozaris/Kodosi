package com.kodosi.mobile.terminals

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.net.Uri
import android.text.Html
import android.text.InputType
import android.view.ActionMode
import android.view.GestureDetector
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.Menu
import android.view.MenuItem
import android.view.MotionEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.core.content.FileProvider
import androidx.core.graphics.withTranslation
import com.kodosi.mobile.R
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Draws a terminal from Ghostty's state and types into it with the keyboard. */
@SuppressLint("ViewConstructor")
class TerminalView(context: Context, private val connection: TerminalConnection, scope: CoroutineScope) :
    View(context) {
    /** Control for the next key, from the key row. */
    var control = false
        set(value) {
            field = value
            onControlChanged(value)
        }
    var onControlChanged: (Boolean) -> Unit = {}
    var onLink: (String) -> Unit = {}
    var onClipboardRead: (((String) -> Boolean) -> Unit) = { it("") }
    var onFailure: () -> Unit = {}
    var onPastePicture: (Uri) -> Unit = {}

    private val regular = resources.getFont(R.font.jetbrains_mono_regular)
    private val bold = resources.getFont(R.font.jetbrains_mono_bold)
    private val italic = resources.getFont(R.font.jetbrains_mono_italic)
    private val boldItalic = resources.getFont(R.font.jetbrains_mono_bold_italic)
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = regular
        textSize = FONT_SIZE_DP * resources.displayMetrics.density
    }
    private val fill = Paint()
    private var cellWidth = ceil(text.measureText("M")).toInt()
    private var cellHeight = ceil(text.fontMetrics.descent - text.fontMetrics.ascent).toInt()
    private var baseline = -text.fontMetrics.ascent
    private val line = max(1, (resources.displayMetrics.density).toInt())
    private val run = StringBuilder()
    private var scrolled = 0f
    private var accent = 0
    private var selection: ActionMode? = null
    private var selecting = false
    private var mouseButton = 1
    private val clipboardWrites = Channel<ByteArray>(Channel.CONFLATED)
    private val frameQueued = AtomicBoolean()
    private val frameUpdate = Runnable {
        frameQueued.set(false)
        clipboard()
        postInvalidateOnAnimation()
    }

    private val gestures = GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(event: MotionEvent) = true

            override fun onSingleTapUp(event: MotionEvent): Boolean {
                val frame = connection.kernel.frame()
                val column = (event.x / cellWidth).toInt()
                val row = ((event.y + (frame?.let(::top) ?: 0)) / cellHeight).toInt()
                if (frame?.mouseTracking == true && event.metaState and KeyEvent.META_SHIFT_ON == 0) {
                    mouse(0, event, 1)
                    mouse(1, event, 1)
                    return true
                }
                (connection.kernel.link(column, row) ?: frame?.link(column, row))?.let {
                    onLink(it)
                    return true
                }
                selection?.finish()
                showKeyboard()
                return true
            }

            override fun onLongPress(event: MotionEvent) {
                val frame = connection.kernel.frame() ?: return
                if (frame.mouseTracking && event.metaState and KeyEvent.META_SHIFT_ON == 0) {
                    mouse(0, event, 2)
                    mouse(1, event, 2)
                    return
                }
                connection.kernel.select(
                    (event.x / cellWidth).toInt(),
                    ((event.y + top(frame)) / cellHeight).toInt(),
                    word = true
                )
                selecting = true
                showSelection()
                invalidate()
            }

            override fun onScroll(first: MotionEvent?, event: MotionEvent, dx: Float, dy: Float): Boolean {
                if (selecting) {
                    val frame = connection.kernel.frame() ?: return true
                    connection.kernel.select(
                        (event.x / cellWidth).toInt(),
                        ((event.y + top(frame)) / cellHeight).toInt(),
                        word = false
                    )
                    invalidate()
                    return true
                }
                scrolled += dy
                val rows = (scrolled / cellHeight).toInt()
                if (rows != 0) {
                    scrolled -= rows * cellHeight
                    val frame = connection.kernel.frame()
                    if (frame?.mouseTracking == true && event.metaState and KeyEvent.META_SHIFT_ON == 0) {
                        repeat(kotlin.math.abs(rows).coerceAtMost(32)) { mouse(0, event, if (rows < 0) 4 else 5) }
                    } else if (connection.kernel.scroll(rows)) {
                        invalidate()
                    }
                }
                return true
            }
        }
    )

    init {
        isFocusable = true
        isFocusableInTouchMode = false
        isFocusedByDefault = true
        isClickable = true
        connection.onChange = {
            if (frameQueued.compareAndSet(false, true) && !post(frameUpdate)) frameQueued.set(false)
        }
        text.letterSpacing = (cellWidth - text.measureText("M")) / text.textSize
        scope.launch {
            for (content in clipboardWrites) writeClipboard(content)
        }
    }

    fun showKeyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java)?.showSoftInput(this, 0)
    }

    fun fontSize(dp: Float) {
        val pixels = dp * resources.displayMetrics.density
        if (text.textSize == pixels) return
        text.textSize = pixels
        text.typeface = regular
        text.letterSpacing = 0f
        cellWidth = ceil(text.measureText("M")).toInt()
        cellHeight = ceil(text.fontMetrics.descent - text.fontMetrics.ascent).toInt()
        baseline = -text.fontMetrics.ascent
        text.letterSpacing = (cellWidth - text.measureText("M")) / text.textSize
        onSizeChanged(width, height, width, height)
        invalidate()
    }

    fun paste(content: String, enter: Boolean = false): Boolean {
        val bytes = connection.kernel.encodePaste(content) ?: return false
        val end = if (enter) connection.kernel.encodeKey(TerminalKernel.Key.ENTER) ?: return false else byteArrayOf()
        return connection.send(bytes + end)
    }

    fun pasteClipboard() {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        val content = clipboard.primaryClip ?: return
        if (content.itemCount == 0) return
        val item = content.getItemAt(0)
        if (item.uri != null && content.description.hasMimeType("image/*")) {
            onPastePicture(item.uri)
            return
        }
        item.coerceToText(context)?.let {
            paste(it.toString())
        }
    }

    override fun performClick(): Boolean {
        super.performClick()
        showKeyboard()
        return true
    }

    private fun clipboard() {
        val content = connection.kernel.clipboard() ?: return
        val kind = content[0].toInt() and 7
        if (kind != 0) clipboardWrites.trySend(content)
        if (content[0].toInt() and 8 != 0) connection.clipboardReader()?.let(onClipboardRead)
    }

    private suspend fun writeClipboard(content: ByteArray) {
        val kind = content[0].toInt() and 7
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        try {
            when (kind) {
                1 -> clipboard.setPrimaryClip(ClipData.newPlainText("", content.decodeToString(1)))

                2 -> {
                    val file = withContext(Dispatchers.IO) {
                        val folder = File(context.cacheDir, "canvas")
                        check(folder.isDirectory || folder.mkdirs())
                        File(folder, "clipboard.png").also { file ->
                            file.outputStream().use { it.write(content, 1, content.size - 1) }
                        }
                    }
                    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
                    clipboard.setPrimaryClip(ClipData.newUri(context.contentResolver, "", uri))
                }

                3 -> {
                    val html = content.decodeToString(1)
                    clipboard.setPrimaryClip(
                        ClipData.newHtmlText("", Html.fromHtml(html, Html.FROM_HTML_MODE_COMPACT), html)
                    )
                }

                4 -> clipboard.clearPrimaryClip()
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Exception) {
            onFailure()
        }
    }

    fun selectAll() {
        connection.kernel.select(-1, -1, word = true)
        showSelection()
        invalidate()
    }

    private fun showSelection() {
        if (selection != null) return
        selection = startActionMode(
            object : ActionMode.Callback {
                override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
                    menu.add(0, 1, 0, R.string.copy)
                    menu.add(0, 2, 1, R.string.paste)
                    return true
                }
                override fun onPrepareActionMode(mode: ActionMode, menu: Menu) = false
                override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
                    if (item.itemId == 1) {
                        connection.kernel.selected()?.let {
                            context.getSystemService(
                                ClipboardManager::class.java
                            ).setPrimaryClip(ClipData.newPlainText("", it))
                        }
                    } else {
                        pasteClipboard()
                    }
                    mode.finish()
                    return true
                }
                override fun onDestroyActionMode(mode: ActionMode) {
                    selection = null
                    selecting = false
                    connection.kernel.clearSelection()
                    invalidate()
                }
            },
            ActionMode.TYPE_FLOATING
        )
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
        if (isFocusableInTouchMode && hasWindowFocus()) requestFocus()
        if (width <= 0 || height <= 0) return
        val cols = max(1, width / cellWidth)
        val rows = max(MINIMUM_ROWS, height / cellHeight)
        connection.setViewport(Viewport(cols, rows, cellWidth, cellHeight))
    }

    override fun onWindowFocusChanged(hasWindowFocus: Boolean) {
        super.onWindowFocusChanged(hasWindowFocus)
        if (hasWindowFocus && isFocusableInTouchMode) requestFocus()
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_UP) selecting = false
        val frame = connection.kernel.frame()
        if (event.getToolType(0) == MotionEvent.TOOL_TYPE_MOUSE &&
            frame?.mouseTracking == true && event.metaState and KeyEvent.META_SHIFT_ON == 0
        ) {
            val action = when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> 0
                MotionEvent.ACTION_UP -> 1
                MotionEvent.ACTION_MOVE -> 2
                else -> return true
            }
            if (action == 0) {
                mouseButton = when {
                    event.buttonState and MotionEvent.BUTTON_SECONDARY != 0 -> 2
                    event.buttonState and MotionEvent.BUTTON_TERTIARY != 0 -> 3
                    else -> 1
                }
            }
            mouse(action, event, mouseButton)
            return true
        }
        return gestures.onTouchEvent(event) || super.onTouchEvent(event)
    }

    private fun mouse(action: Int, event: MotionEvent, button: Int) {
        val frame = connection.kernel.frame() ?: return
        val modifiers = (if (event.metaState and KeyEvent.META_SHIFT_ON != 0) TerminalKernel.Modifier.SHIFT else 0) or
            (if (event.metaState and KeyEvent.META_CTRL_ON != 0) TerminalKernel.Modifier.CONTROL else 0) or
            (if (event.metaState and KeyEvent.META_ALT_ON != 0) TerminalKernel.Modifier.ALT else 0)
        connection.kernel.mouse(
            action, event.x, event.y + top(frame), width, frame.rows * cellHeight,
            cellWidth, cellHeight, button, modifiers
        )?.let(connection::send)
    }

    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (event.actionMasked != MotionEvent.ACTION_SCROLL) return super.onGenericMotionEvent(event)
        val rows = event.getAxisValue(MotionEvent.AXIS_VSCROLL)
        val frame = connection.kernel.frame() ?: return false
        if (frame.mouseTracking) {
            repeat(kotlin.math.abs(rows).toInt().coerceIn(1, 32)) { mouse(0, event, if (rows > 0) 4 else 5) }
        } else if (connection.kernel.scroll((-rows * 3).toInt())) {
            invalidate()
        }
        return true
    }

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
        val images = connection.kernel.images()
        canvas.withTranslation(0f, -top(frame).toFloat()) {
            fun pictures(layer: Int) {
                images.filter { it.layer == layer }.forEach {
                    drawBitmap(it.bitmap, it.source, it.destination(cellWidth, cellHeight), null)
                }
            }
            pictures(0)
            for (row in 0 until frame.rows) drawBackgrounds(this, frame, row)
            pictures(1)
            for (row in 0 until frame.rows) drawText(this, frame, row)
            pictures(2)
            drawCursor(this, frame)
        }
        if (connection.kernel.animationDelay >=
            0
        ) {
            postInvalidateDelayed(connection.kernel.animationDelay.coerceAtLeast(8))
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
            val color = background(frame, column, row)
            var end = column + 1
            while (end < frame.cols && background(frame, end, row) == color) end++
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

    private fun background(frame: TerminalFrame, column: Int, row: Int): Int =
        if (frame.style(column, row) and TerminalFrame.Style.SELECTED != 0) {
            0xFF5C3825.toInt()
        } else {
            frame.background(column, row)
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
