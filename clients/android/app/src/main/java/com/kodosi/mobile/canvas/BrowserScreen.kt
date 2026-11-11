package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.SurfaceTexture
import android.text.InputType
import android.view.GestureDetector
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.Surface
import android.view.TextureView
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.PopupProperties
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.flag
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.DarkColors
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiSpinner
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.well
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

@Composable
fun BrowserScreen(model: AppModel, session: String, browser: CanvasBrowser) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    val history by model.preferences.browserHistory.collectAsStateWithLifecycle()
    val feed = remember(session) { BrowserFeed(model.canvas, session, scope) }
    val window by feed.window.collectAsStateWithLifecycle()
    val picture by feed.picture.collectAsStateWithLifecycle()
    var address by remember { mutableStateOf(browser.tabs.firstOrNull { it.active }?.url.orEmpty()) }
    var edited by remember { mutableStateOf(false) }
    var addressFocused by remember { mutableStateOf(false) }
    var view by remember { mutableStateOf<BrowserSurface?>(null) }
    var dialogText by remember { mutableStateOf("") }
    val feedTabs = window.objects("tabs")
    val tabs = remember(feedTabs, browser.tabs) {
        feedTabs.map {
            BrowserTab(
                it.string("id").orEmpty(),
                it.string("title").orEmpty(),
                it.string("address").orEmpty(),
                it.flag("active")
            )
        }.ifEmpty { browser.tabs }
    }
    val awake = browser.state != "asleep" && window.isNotEmpty()
    val dialog = window["dialog"] as? JsonObject
    val menu = window["menu"] as? JsonObject
    val chooser = window["chooser"] as? JsonObject
    var picking by remember { mutableStateOf(false) }
    var several by remember { mutableStateOf(false) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { files ->
        scope.launch {
            val paths = mutableListOf<String>()
            model.runAction {
                for (file in files.take(if (several) files.size else 1)) {
                    val content = readPickedFile(context, file)
                    model.canvas.attach(session, content.first, content.second, page = true)?.let(paths::add)
                }
            }
            feed.input(fields("kind" to "files", "paths" to paths))
            picking = false
        }
    }
    LaunchedEffect(tabs, feedTabs) {
        if (!edited) address = tabs.firstOrNull { it.active }?.url ?: address
        feedTabs.firstOrNull { it.flag("active") && !it.flag("loading") }?.let {
            model.preferences.browsed(it.string("title").orEmpty(), it.string("address").orEmpty())
        }
    }
    LaunchedEffect(dialog) { dialogText = dialog?.string("text").orEmpty() }
    LaunchedEffect(window.string("copied")) {
        window.string("copied")?.let {
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("", it))
        }
    }
    DisposableEffect(feed) { onDispose { feed.close() } }
    LifecycleResumeEffect(feed) {
        feed.show(true)
        onPauseOrDispose { feed.show(false) }
    }
    val colors = KodosiTheme.colors
    val suggestions = history.filter {
        val query = if (edited) address.trim() else ""
        query.isEmpty() || it.address.contains(query, ignoreCase = true) || it.title.contains(query, ignoreCase = true)
    }.take(6)
    val open: (String) -> Unit = {
        address = it
        feed.input(fields("kind" to "open", "address" to it))
        edited = false
        addressFocused = false
        focus.clearFocus()
        keyboard?.hide()
        view?.requestFocus()
    }
    Column(Modifier.fillMaxSize().imePadding()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            LazyRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                items(tabs, key = { it.id }) { tab ->
                    CanvasTab(
                        tab.title.ifBlank { tab.url }.ifBlank { stringResource(R.string.browser) },
                        tab.active,
                        choose = { feed.input(fields("kind" to "tab", "id" to tab.id)) },
                        icon = {
                            if (feedTabs.any { it.string("id") == tab.id && it.flag("loading") }) {
                                KodosiSpinner(Modifier.size(12.dp))
                            } else {
                                AppIcon(AppSymbol.BROWSER, Modifier.size(12.dp), tint = colors.accent)
                            }
                        },
                        close = if (tabs.size > 1) {
                            (
                                {
                                    feed.input(fields("kind" to "closeTab", "id" to tab.id))
                                }
                                )
                        } else {
                            null
                        }
                    )
                }
            }
            ActionIcon(
                AppSymbol.ADD,
                R.string.new_browser_tab,
                { feed.input(fields("kind" to "newTab")) },
                enabled = awake
            )
        }
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            ActionIcon(AppSymbol.BACK, R.string.browser_back, { feed.input(fields("kind" to "back")) }, enabled = awake)
            ActionIcon(AppSymbol.FORWARD, R.string.browser_forward, {
                feed.input(fields("kind" to "forward"))
            }, enabled = awake)
            ActionIcon(AppSymbol.REFRESH, R.string.browser_reload, {
                feed.input(fields("kind" to "reload"))
            }, enabled = awake)
            Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                Box(Modifier.widthIn(max = 620.dp).fillMaxWidth()) {
                    val addressLabel = stringResource(R.string.browser_address)
                    BasicTextField(
                        address,
                        {
                            address = it
                            edited = true
                        },
                        singleLine = true,
                        enabled = awake,
                        textStyle = KodosiType.footnote.copy(color = colors.ink),
                        cursorBrush = SolidColor(colors.accent),
                        modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).well(colors, CircleShape)
                            .border(
                                1.5.dp,
                                if (addressFocused) colors.accent.copy(alpha = 0.55f) else Color.Transparent,
                                CircleShape
                            )
                            .padding(horizontal = 10.dp, vertical = 9.dp)
                            .onFocusChanged { addressFocused = it.isFocused }
                            .semantics { contentDescription = addressLabel },
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Uri,
                            autoCorrectEnabled = false,
                            imeAction = ImeAction.Go
                        ),
                        keyboardActions = KeyboardActions(onGo = { open(address) }),
                        decorationBox = { input ->
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(7.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                AppIcon(
                                    if (addressFocused) {
                                        AppSymbol.SEARCH
                                    } else if (address.startsWith(
                                            "https://",
                                            true
                                        )
                                    ) {
                                        AppSymbol.LOCK
                                    } else {
                                        AppSymbol.BROWSER
                                    },
                                    Modifier.size(11.dp),
                                    tint = colors.inkFaint
                                )
                                Box(Modifier.weight(1f)) {
                                    if (address.isEmpty()) {
                                        Text(
                                            addressLabel,
                                            style = KodosiType.footnote,
                                            color = colors.inkMuted,
                                            maxLines = 1
                                        )
                                    }
                                    input()
                                }
                            }
                        }
                    )
                    KodosiMenu(
                        expanded = addressFocused && suggestions.isNotEmpty(),
                        onDismissRequest = { addressFocused = false },
                        modifier = Modifier.heightIn(max = 320.dp),
                        properties = PopupProperties(focusable = false)
                    ) {
                        suggestions.forEach { page ->
                            KodosiMenuItem(text = {
                                Column {
                                    Text(page.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Text(
                                        page.address,
                                        style = KodosiType.caption,
                                        color = colors.inkMuted,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis
                                    )
                                }
                            }, onClick = { open(page.address) })
                        }
                    }
                }
            }
            ActionIcon(AppSymbol.KEYBOARD, R.string.keyboard, { view?.keyboard() }, enabled = awake)
        }
        window.objects("downloads").firstOrNull { !it.flag("done") }?.let {
            ProgressCaption(it.string("name").orEmpty())
        }
        Box(Modifier.weight(1f).fillMaxWidth().background(DarkColors.terminal)) {
            AndroidView(
                factory = { BrowserSurface(it, feed).also { created -> view = created } },
                update = { it.window = window },
                modifier = Modifier.fillMaxSize()
            )
            BrowserPointers(window, browser, feed)
            KodosiTheme(dark = true) {
                if (browser.state == "asleep") {
                    EmptyState(stringResource(R.string.browser_sleeping), browser.problem, AppSymbol.BROWSER) {
                        KodosiTextButton(onClick = {
                            scope.launch { model.runAction { model.canvas.wake(session) } }
                        }) { Text(stringResource(R.string.wake_browser)) }
                    }
                } else if (!picture) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                        ProgressCaption(stringResource(R.string.loading))
                    }
                }
            }
        }
    }
    if (dialog != null) {
        KodosiDialog(
            onDismissRequest = { feed.input(fields("kind" to "dialog", "accept" to false)) },
            title = { Text(stringResource(R.string.browser_dialog)) },
            text = {
                Column {
                    Text(dialog.string("message").orEmpty())
                    if (dialog.string("kind") == "prompt") KodosiField(dialogText, { dialogText = it })
                }
            },
            confirmButton = {
                KodosiTextButton(onClick = {
                    feed.input(
                        fields(
                            "kind" to "dialog",
                            "accept" to true,
                            "text" to dialogText
                        )
                    )
                }) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = {
                KodosiTextButton(onClick = {
                    feed.input(fields("kind" to "dialog", "accept" to false))
                }) { Text(stringResource(R.string.cancel)) }
            }
        )
    }
    if (menu != null) {
        KodosiDialog(
            onDismissRequest = { feed.input(fields("kind" to "choose", "menu" to menu.number("id"))) },
            title = { Text(stringResource(R.string.browser_menu)) },
            text = {
                Column {
                    menu.objects("items").forEachIndexed { index, item ->
                        KodosiTextButton(onClick = {
                            feed.input(
                                fields(
                                    "kind" to "choose",
                                    "menu" to menu.number("id"),
                                    "choice" to index
                                )
                            )
                        }, enabled = item.flag("enabled")) { Text(item.string("label").orEmpty()) }
                    }
                }
            },
            confirmButton = {
                KodosiTextButton(onClick = {
                    feed.input(fields("kind" to "choose", "menu" to menu.number("id")))
                }) { Text(stringResource(R.string.cancel)) }
            }
        )
    }
    if (chooser != null && !picking) {
        KodosiDialog(
            onDismissRequest = { feed.input(fields("kind" to "files", "paths" to emptyList<String>())) },
            title = { Text(stringResource(R.string.choose_file)) },
            confirmButton = {
                KodosiTextButton(onClick = {
                    several = chooser.flag("several")
                    picking = true
                    picker.launch(arrayOf("*/*"))
                }) { Text(stringResource(R.string.choose_file)) }
            },
            dismissButton = {
                KodosiTextButton(onClick = {
                    feed.input(fields("kind" to "files", "paths" to emptyList<String>()))
                }) { Text(stringResource(R.string.cancel)) }
            }
        )
    }
}

@SuppressLint("ViewConstructor")
internal class BrowserSurface(context: Context, private val feed: BrowserFeed) :
    FrameLayout(context),
    TextureView.SurfaceTextureListener {
    private val texture = TextureView(context).also {
        addView(it, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        it.surfaceTextureListener =
            this
    }
    private var surface: Surface? = null
    private var pressed = "left"
    private var accent = 0
    var window = fields()
        set(value) {
            field = value
            fit()
        }
    private val gestures = GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(event: MotionEvent) = true
            override fun onSingleTapUp(event: MotionEvent): Boolean {
                mouse("press", event.x, event.y, "left")
                mouse("release", event.x, event.y, "left")
                requestFocus()
                return true
            }
            override fun onLongPress(event: MotionEvent) {
                mouse("press", event.x, event.y, "right")
                mouse("release", event.x, event.y, "right")
            }
            override fun onScroll(first: MotionEvent?, event: MotionEvent, dx: Float, dy: Float): Boolean {
                mouse("wheel", event.x, event.y, dx = dx, dy = dy)
                return true
            }
        }
    )

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    private fun fit() {
        val bw = window.number("width")?.toInt() ?: width
        val bh = window.number("height")?.toInt() ?: height
        if (bw <= 0 || bh <= 0 || width <= 0 || height <= 0) return
        val fit = minOf(width.toFloat() / bw, height.toFloat() / bh)
        val w = (bw * fit).toInt()
        val h = (bh * fit).toInt()
        if (texture.layoutParams.width != w || texture.layoutParams.height != h) {
            texture.layoutParams = LayoutParams(w, h, android.view.Gravity.CENTER)
        }
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        fit()
        feed.resize(w, h, resources.displayMetrics.density)
    }
    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
        surface = Surface(texture).also { feed.surface(it, this.width, this.height, resources.displayMetrics.density) }
    }
    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = Unit
    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) = feed.rendered()
    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
        feed.surface(null, 0, 0, 1f)
        surface?.release()
        surface =
            null
        return true
    }

    private fun mouse(
        action: String,
        x: Float,
        y: Float,
        button: String = "none",
        dx: Float = 0f,
        dy: Float = 0f,
        modifiers: Int = 0
    ) {
        val bw = window.number("width")?.toFloat() ?: return
        val bh = window.number("height")?.toFloat() ?: return
        if (bw <= 0 || bh <= 0) return
        val fit = minOf(width / bw, height / bh)
        val rawX = (x - (width - bw * fit) / 2) / fit
        val rawY = (y - (height - bh * fit) / 2) / fit
        if ((rawX !in 0f..bw || rawY !in 0f..bh) &&
            action != "release" && !(action == "move" && button != "none")
        ) {
            return
        }
        val px = rawX.coerceIn(0f, (bw - 1).coerceAtLeast(0f))
        val py = rawY.coerceIn(0f, (bh - 1).coerceAtLeast(0f))
        feed.input(
            fields(
                "kind" to "mouse",
                "action" to action,
                "x" to px,
                "y" to py,
                "button" to button,
                "clicks" to 1,
                "deltaX" to (dx / fit),
                "deltaY" to (dy / fit), "modifiers" to modifiers
            )
        )
    }

    @android.annotation.SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.getToolType(0) != MotionEvent.TOOL_TYPE_MOUSE) return gestures.onTouchEvent(event)
        if (event.actionMasked == MotionEvent.ACTION_DOWN) {
            requestFocus()
            pressed = when {
                event.buttonState and MotionEvent.BUTTON_SECONDARY != 0 -> "right"
                event.buttonState and MotionEvent.BUTTON_TERTIARY != 0 -> "middle"
                else -> "left"
            }
        }
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> "press"
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> "release"
            MotionEvent.ACTION_MOVE -> "move"
            else -> return true
        }
        mouse(action, event.x, event.y, pressed, modifiers = modifiers(event.metaState))
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

    fun keyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java).showSoftInput(this, 0)
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
            override fun sendKeyEvent(event: KeyEvent): Boolean = this@BrowserSurface.dispatchKeyEvent(event)
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
            (if (event.isAltPressed) 1 else 0) or (if (event.isCtrlPressed) 2 else 0) or
                (if (event.isMetaPressed) 4 else 0) or
                (if (event.isShiftPressed) 8 else 0)
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
            key(text, "Key${text.uppercase()}", text.uppercase().first().code, modifiers)
        }
        return true
    }
}
