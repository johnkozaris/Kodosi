package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.PointF
import android.text.InputType
import android.view.GestureDetector
import android.view.HapticFeedbackConstants
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.OverScroller
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.runtime.Composable
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppSymbol
import kotlin.math.hypot

// Touches, a mouse and a keyboard on a stream's video: a tap clicks, a long press clicks with the right
// button or drags, a pan scrolls with momentum, and two fingers tap with the right button.
internal abstract class StreamInput(context: Context, private val feed: VideoFeed) : FrameLayout(context) {
    private var pressed = "left"
    private var clickCount = 0
    private var clickButton = ""
    private var clickX = 0f
    private var clickY = 0f
    private var releasedAt = 0L
    private val doubleTapSlop = ViewConfiguration.get(context).scaledDoubleTapSlop
    private val touchSlop = ViewConfiguration.get(context).scaledTouchSlop
    private var held = false
    private var dragging = false
    private var holdX = 0f
    private var holdY = 0f
    private var pairAt = -1L
    private var pairX = 0f
    private var pairY = 0f
    private val fling = OverScroller(context)
    private var flungX = 0
    private var flungY = 0
    private var flingAtX = 0f
    private var flingAtY = 0f
    private var touchStoppedFling = false
    private val flingStep = Runnable { flingFrame() }
    private var accent = 0
    private val gestures = GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(event: MotionEvent) = true
            override fun onSingleTapUp(event: MotionEvent): Boolean {
                if (touchStoppedFling) return true
                countClick("left", event.x, event.y, event.downTime)
                mouse("press", event.x, event.y, "left", clickCount)
                mouse("release", event.x, event.y, "left", clickCount)
                releasedAt = event.eventTime
                requestFocus()
                return true
            }
            override fun onLongPress(event: MotionEvent) {
                held = true
                dragging = false
                holdX = event.x
                holdY = event.y
                clickButton = ""
                performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            }
            override fun onScroll(first: MotionEvent?, event: MotionEvent, dx: Float, dy: Float): Boolean {
                if (scrolls(event)) mouse("wheel", event.x, event.y, dx = dx, dy = dy)
                return true
            }
            override fun onFling(first: MotionEvent?, event: MotionEvent, velocityX: Float, velocityY: Float): Boolean {
                if (!scrolls(event)) return true
                flingAtX = event.x
                flingAtY = event.y
                flungX = 0
                flungY = 0
                fling.fling(
                    0,
                    0,
                    -velocityX.toInt(),
                    -velocityY.toInt(),
                    Int.MIN_VALUE,
                    Int.MAX_VALUE,
                    Int.MIN_VALUE,
                    Int.MAX_VALUE
                )
                postOnAnimation(flingStep)
                return true
            }
        }
    ).apply {
        // Disable double-tap detection so the second tap still reaches onSingleTapUp.
        setOnDoubleTapListener(null)
    }

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        defaultFocusHighlightEnabled = false
    }

    // The stream's point under a view point, kept to the stream's edge when `clamped`, or null
    // outside the stream.
    protected abstract fun streamPoint(x: Float, y: Float, clamped: Boolean): PointF?

    // View pixels per stream point, so that a scroll follows the finger.
    protected abstract fun fit(): Float?

    // Whether a touch pan scrolls the stream.
    protected open fun scrolls(event: MotionEvent) = true

    // Hands this device's clipboard to the stream before the keys of a paste, and says whether the stream
    // takes it.
    protected open fun paste(): Boolean = false

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        feed.resize(w, h, resources.displayMetrics.density)
    }

    private fun countClick(button: String, x: Float, y: Float, pressedAt: Long) {
        val again = button == clickButton && pressedAt - releasedAt <= ViewConfiguration.getDoubleTapTimeout() &&
            hypot(x - clickX, y - clickY) <= doubleTapSlop
        clickCount = if (again) clickCount + 1 else 1
        clickButton = button
        clickX = x
        clickY = y
    }

    // Releases and drags that leave the stream still reach it, at its edge.
    protected fun mouse(
        action: String,
        x: Float,
        y: Float,
        button: String = "none",
        clicks: Int = 0,
        dx: Float = 0f,
        dy: Float = 0f,
        modifiers: Int = 0
    ): Boolean {
        val fit = fit()?.takeIf { it > 0 } ?: return false
        val point = streamPoint(x, y, clamped = action == "release" || (action == "move" && button != "none"))
            ?: return false
        feed.input(
            fields(
                "kind" to "mouse",
                "action" to action,
                "x" to point.x,
                "y" to point.y,
                "button" to button,
                "clicks" to clicks,
                "deltaX" to (dx / fit),
                "deltaY" to (dy / fit), "modifiers" to modifiers
            )
        )
        return true
    }

    private fun rightClick(x: Float, y: Float) {
        if (mouse("press", x, y, "right", 1)) mouse("release", x, y, "right", 1)
    }

    // Touch gesture mapping: long-press + drag sends a left-button drag (for text selection);
    // long-press + release or a two-finger tap sends a right-click to open the context menu.
    private fun touch(event: MotionEvent) {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                touchStoppedFling = !fling.isFinished
                stopFling()
            }

            MotionEvent.ACTION_POINTER_DOWN -> {
                if (dragging) mouse("release", event.x, event.y, "left", 1)
                pairAt = if (event.pointerCount == 2 && !held) event.eventTime else -1L
                pairX = (event.getX(0) + event.getX(1)) / 2
                pairY = (event.getY(0) + event.getY(1)) / 2
                held = false
                dragging = false
            }

            MotionEvent.ACTION_POINTER_UP -> {
                if (pairAt >= 0 && event.eventTime - pairAt <= ViewConfiguration.getLongPressTimeout()) {
                    rightClick(pairX, pairY)
                }
                pairAt = -1L
            }

            MotionEvent.ACTION_MOVE -> {
                if (pairAt >= 0 && event.pointerCount >= 2) {
                    val x = (event.getX(0) + event.getX(1)) / 2
                    val y = (event.getY(0) + event.getY(1)) / 2
                    if (hypot(x - pairX, y - pairY) > touchSlop) pairAt = -1L
                }
                if (held && !dragging && hypot(event.x - holdX, event.y - holdY) > touchSlop) {
                    dragging = mouse("press", holdX, holdY, "left", 1)
                    held = dragging
                }
                if (dragging) mouse("move", event.x, event.y, "left")
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (dragging) {
                    mouse("release", event.x, event.y, "left", 1)
                } else if (held && event.actionMasked == MotionEvent.ACTION_UP) {
                    rightClick(holdX, holdY)
                }
                held = false
                dragging = false
                pairAt = -1L
            }
        }
    }

    private fun flingFrame() {
        if (!fling.computeScrollOffset()) return
        val dx = fling.currX - flungX
        val dy = fling.currY - flungY
        flungX = fling.currX
        flungY = fling.currY
        if (dx != 0 || dy != 0) mouse("wheel", flingAtX, flingAtY, dx = dx.toFloat(), dy = dy.toFloat())
        postOnAnimation(flingStep)
    }

    private fun stopFling() {
        fling.forceFinished(true)
        removeCallbacks(flingStep)
    }

    override fun onDetachedFromWindow() {
        stopFling()
        super.onDetachedFromWindow()
    }

    @android.annotation.SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.getToolType(0) != MotionEvent.TOOL_TYPE_MOUSE) {
            touch(event)
            return gestures.onTouchEvent(event)
        }
        if (event.actionMasked == MotionEvent.ACTION_DOWN) {
            requestFocus()
            pressed = when {
                event.buttonState and MotionEvent.BUTTON_SECONDARY != 0 -> "right"
                event.buttonState and MotionEvent.BUTTON_TERTIARY != 0 -> "middle"
                else -> "left"
            }
            countClick(pressed, event.x, event.y, event.eventTime)
        }
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> "press"
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> "release"
            MotionEvent.ACTION_MOVE -> "move"
            else -> return true
        }
        if (action == "release") releasedAt = event.eventTime
        val clicks = if (action == "move") 0 else clickCount
        mouse(action, event.x, event.y, pressed, clicks, modifiers = modifiers(event.metaState))
        return true
    }
    override fun onHoverEvent(event: MotionEvent): Boolean {
        mouse("move", event.x, event.y, modifiers = modifiers(event.metaState))
        return true
    }
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (event.action == MotionEvent.ACTION_SCROLL) {
            mouse(
                "wheel",
                event.x,
                event.y,
                dx = -event.getAxisValue(MotionEvent.AXIS_HSCROLL) * 40,
                dy =
                    -event.getAxisValue(MotionEvent.AXIS_VSCROLL) * 40,
                modifiers = modifiers(event.metaState)
            )
            return true
        }
        return super.onGenericMotionEvent(event)
    }

    fun showKeyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java).showSoftInput(this, 0)
    }

    fun hideKeyboard() {
        context.getSystemService(InputMethodManager::class.java).hideSoftInputFromWindow(windowToken, 0)
    }

    private fun modifiers(state: Int): Int = (if (state and KeyEvent.META_ALT_MASK != 0) 1 else 0) or
        (if (state and KeyEvent.META_CTRL_MASK != 0) 2 else 0) or
        (if (state and KeyEvent.META_META_MASK != 0) 4 else 0) or
        (if (state and KeyEvent.META_SHIFT_MASK != 0) 8 else 0)
    override fun onCheckIsTextEditor() = true
    override fun onCreateInputConnection(out: EditorInfo): InputConnection {
        out.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
        out.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN
        return object : BaseInputConnection(this, false) {
            private var composing = ""
            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
                composing = ""
                feed.input(fields("kind" to "text", "text" to text.toString()))
                return true
            }
            override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
                composing =
                    text.toString()
                return true
            }
            override fun finishComposingText(): Boolean {
                if (composing.isNotEmpty()) commitText(composing, 1)
                return true
            }
            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                repeat(beforeLength.coerceAtMost(1024)) { key("Backspace", "Backspace", 8) }
                repeat(afterLength.coerceAtMost(1024)) { key("Delete", "Delete", 46) }
                return true
            }

            // A paste from the keyboard's menu, which has no keys of its own.
            override fun performContextMenuAction(id: Int): Boolean {
                if (id != android.R.id.paste && id != android.R.id.pasteAsPlainText) return false
                if (paste()) key("v", "KeyV", 86, CONTROL)
                return true
            }
            override fun sendKeyEvent(event: KeyEvent): Boolean = this@StreamInput.dispatchKeyEvent(event)
        }
    }

    private fun key(value: String, code: String, number: Int, modifiers: Int = 0) {
        feed.input(
            fields(
                "kind" to "key",
                "action" to "down",
                "key" to value,
                "code" to code,
                "keyCode" to number,
                "modifiers" to modifiers
            )
        )
        feed.input(
            fields(
                "kind" to "key",
                "action" to "up",
                "key" to value,
                "code" to code,
                "keyCode" to number,
                "modifiers" to modifiers
            )
        )
    }
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val named = when (keyCode) {
            KeyEvent.KEYCODE_ENTER -> Triple("Enter", "Enter", 13)
            KeyEvent.KEYCODE_DEL -> Triple("Backspace", "Backspace", 8)
            KeyEvent.KEYCODE_FORWARD_DEL -> Triple("Delete", "Delete", 46)
            KeyEvent.KEYCODE_TAB -> Triple("Tab", "Tab", 9)
            KeyEvent.KEYCODE_ESCAPE -> Triple("Escape", "Escape", 27)
            KeyEvent.KEYCODE_DPAD_LEFT -> Triple("ArrowLeft", "ArrowLeft", 37)
            KeyEvent.KEYCODE_DPAD_UP -> Triple("ArrowUp", "ArrowUp", 38)
            KeyEvent.KEYCODE_DPAD_RIGHT -> Triple("ArrowRight", "ArrowRight", 39)
            KeyEvent.KEYCODE_DPAD_DOWN -> Triple("ArrowDown", "ArrowDown", 40)
            else -> null
        }
        val modifiers =
            (if (event.isAltPressed) 1 else 0) or (if (event.isCtrlPressed) CONTROL else 0) or
                (if (event.isMetaPressed) 4 else 0) or
                (if (event.isShiftPressed) 8 else 0)
        if (keyCode == KeyEvent.KEYCODE_PASTE) {
            if (!paste()) return super.onKeyDown(keyCode, event)
            key("v", "KeyV", 86, CONTROL)
            return true
        }
        if (named != null) {
            key(named.first, named.second, named.third, modifiers)
            return true
        }
        val char = event.getUnicodeChar(
            event.metaState and (KeyEvent.META_CTRL_MASK or KeyEvent.META_ALT_MASK or KeyEvent.META_META_MASK).inv()
        )
        if (char == 0) return super.onKeyDown(keyCode, event)
        if (char and KeyCharacterMap.COMBINING_ACCENT != 0) {
            accent = char and KeyCharacterMap.COMBINING_ACCENT_MASK
            return true
        }
        val composed = KeyCharacterMap.getDeadChar(accent, char).takeIf { accent != 0 && it != 0 }
        accent = 0
        val text = String(Character.toChars(composed ?: char))
        if (modifiers and 7 ==
            0
        ) {
            feed.input(fields("kind" to "text", "text" to text))
        } else {
            if (modifiers == CONTROL && text == "v") paste()
            key(text, "Key${text.uppercase()}", text.uppercase().first().code, modifiers)
        }
        return true
    }

    private companion object {
        const val CONTROL = 2
    }
}

@Composable
internal fun KeyboardButton(input: StreamInput?, enabled: Boolean) {
    if (WindowInsets.isImeVisible) {
        ActionIcon(AppSymbol.HIDE_KEYBOARD, R.string.hide_keyboard, { input?.hideKeyboard() })
    } else {
        ActionIcon(AppSymbol.KEYBOARD, R.string.keyboard, { input?.showKeyboard() }, enabled = enabled)
    }
}
