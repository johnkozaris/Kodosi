package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.SurfaceTexture
import android.text.InputType
import android.view.GestureDetector
import android.view.HapticFeedbackConstants
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.Surface
import android.view.TextureView
import android.view.ViewConfiguration
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.OverScroller
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
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
import com.kodosi.mobile.ui.ReconnectingNotice
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.well
import java.text.NumberFormat
import kotlin.math.hypot
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

private enum class BrowserCover { ASLEEP, LOADING }

@Composable
fun BrowserScreen(model: AppModel, session: String, browser: CanvasBrowser, ask: ((String?) -> Unit)? = null) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    val history by model.preferences.browserHistory.collectAsStateWithLifecycle()
    val feed = remember(session) { BrowserFeed(model.browser, session, scope) }
    val window by feed.window.collectAsStateWithLifecycle()
    val icons by feed.icons.collectAsStateWithLifecycle()
    val picture by feed.picture.collectAsStateWithLifecycle()
    val pictureSize by feed.pictureSize.collectAsStateWithLifecycle()
    val reconnecting by feed.reconnecting.collectAsStateWithLifecycle()
    val failure by feed.failure.collectAsStateWithLifecycle()
    val address = remember { BrowserAddress(browser.tabs.firstOrNull { it.active }?.url.orEmpty()) }
    val addressFocus = remember { FocusRequester() }
    var addressFocused by remember { mutableStateOf(false) }
    var view by remember { mutableStateOf<BrowserSurface?>(null) }
    val dialogText = rememberTextFieldState()
    var explaining by remember { mutableStateOf(false) }
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
    val strip = rememberLazyListState()
    val active = tabs.indexOfFirst { it.active }
    LaunchedEffect(active, tabs.size, tabs.getOrNull(active)) {
        if (active >= 0) strip.keepInView(active)
    }
    LaunchedEffect(tabs, feedTabs) {
        tabs.firstOrNull { it.active }?.url?.let(address::follow)
        feedTabs.firstOrNull { it.flag("active") && !it.flag("loading") }?.let {
            model.preferences.browsed(it.string("title").orEmpty(), it.string("address").orEmpty())
        }
    }
    LaunchedEffect(dialog) { dialogText.setTextAndPlaceCursorAtEnd(dialog?.string("text").orEmpty()) }
    LaunchedEffect(feed) {
        feed.copies.collect {
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
        val query = if (address.edited) address.state.text.toString().trim() else ""
        query.isEmpty() || it.address.contains(query, ignoreCase = true) || it.title.contains(query, ignoreCase = true)
    }.take(6)
    val open: (String) -> Unit = {
        address.place(it)
        feed.input(fields("kind" to "open", "address" to it))
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
            LazyRow(Modifier.weight(1f), state = strip, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                items(tabs, key = { it.id }) { tab ->
                    CanvasTab(
                        tab.title.ifBlank { tab.url }.ifBlank { stringResource(R.string.browser) },
                        tab.active,
                        choose = { feed.input(fields("kind" to "tab", "id" to tab.id)) },
                        icon = {
                            val icon = icons[tab.id]
                            if (feedTabs.any { it.string("id") == tab.id && it.flag("loading") }) {
                                KodosiSpinner(Modifier.size(12.dp))
                            } else if (icon != null) {
                                Image(icon, null, Modifier.size(14.dp).clip(RoundedCornerShape(3.dp)))
                            } else {
                                AppIcon(AppSymbol.BROWSER, Modifier.size(12.dp), tint = colors.accent)
                            }
                        },
                        close = { feed.input(fields("kind" to "closeTab", "id" to tab.id)) }
                    )
                }
            }
            ActionIcon(
                AppSymbol.ADD,
                R.string.new_browser_tab,
                {
                    feed.input(fields("kind" to "newTab"))
                    address.place("")
                    addressFocus.requestFocus()
                    keyboard?.show()
                },
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
            Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                Box(Modifier.widthIn(max = 620.dp).fillMaxWidth()) {
                    val addressLabel = stringResource(R.string.browser_address)
                    BasicTextField(
                        address.state,
                        lineLimits = TextFieldLineLimits.SingleLine,
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
                            .focusRequester(addressFocus)
                            .onFocusChanged { addressFocused = it.isFocused }
                            .semantics { contentDescription = addressLabel },
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Uri,
                            autoCorrectEnabled = false,
                            imeAction = ImeAction.Go
                        ),
                        onKeyboardAction = { open(address.state.text.toString()) },
                        decorator = { input ->
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(7.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                AppIcon(
                                    if (addressFocused) {
                                        AppSymbol.SEARCH
                                    } else if (address.state.text.startsWith(
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
                                    if (address.state.text.isEmpty()) {
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
            if (browser.ownSignIns) {
                Row(
                    Modifier.clip(CircleShape).background(colors.ink.copy(alpha = 0.06f))
                        .clickable(role = Role.Button) { explaining = true }
                        .heightIn(min = 36.dp).padding(horizontal = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    AppIcon(AppSymbol.FRIENDS, Modifier.size(11.dp), tint = colors.inkMuted)
                    Text(stringResource(R.string.browser_shared), style = KodosiType.footnote, color = colors.inkMuted)
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
                update = {
                    it.window = window
                    it.pictureSize = pictureSize
                },
                modifier = Modifier.fillMaxSize()
            )
            BrowserPointers(window, browser, feed)
            KodosiTheme(dark = true) {
                val fading = fade<Float>()
                val cover = when {
                    browser.state == "asleep" -> BrowserCover.ASLEEP
                    !picture -> BrowserCover.LOADING
                    else -> null
                }
                AnimatedContent(
                    cover,
                    Modifier.fillMaxSize(),
                    transitionSpec = { (fadeIn(fading) togetherWith fadeOut(fading)).using(null) },
                    label = "browser cover"
                ) { shown ->
                    when (shown) {
                        BrowserCover.ASLEEP -> EmptyState(
                            stringResource(R.string.browser_sleeping),
                            browser.problem,
                            AppSymbol.BROWSER
                        ) {
                            KodosiTextButton(onClick = {
                                model.browser.choose(session)
                                scope.launch { model.runAction { model.canvas.wake(session) } }
                            }) { Text(stringResource(R.string.wake_browser)) }
                        }

                        BrowserCover.LOADING -> Column(
                            Modifier.fillMaxSize().padding(24.dp),
                            verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterVertically),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            ProgressCaption(stringResource(R.string.loading))
                            browser.getting?.let { getting ->
                                val locale = LocalConfiguration.current.locales[0]
                                val percent = remember(getting, locale) {
                                    NumberFormat.getPercentInstance(locale).format(getting / 100.0)
                                }
                                Text(percent, style = KodosiType.footnote, color = KodosiTheme.colors.inkMuted)
                            }
                            failure?.let {
                                Text(
                                    it,
                                    Modifier.widthIn(max = 320.dp),
                                    style = KodosiType.footnote,
                                    color = KodosiTheme.colors.inkMuted,
                                    textAlign = TextAlign.Center
                                )
                            }
                        }

                        null -> Unit
                    }
                }
                ReconnectingNotice(
                    cover == null && reconnecting,
                    Modifier.align(Alignment.TopCenter).padding(top = 10.dp)
                )
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
                    if (dialog.string("kind") == "prompt") KodosiField(dialogText)
                }
            },
            confirmButton = {
                KodosiTextButton(onClick = {
                    feed.input(
                        fields(
                            "kind" to "dialog",
                            "accept" to true,
                            "text" to dialogText.text.toString()
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
    if (explaining) {
        KodosiDialog(
            onDismissRequest = { explaining = false },
            title = { Text(stringResource(R.string.browser_shared)) },
            text = { Text(stringResource(R.string.browser_shared_explained)) },
            confirmButton = {
                KodosiTextButton(onClick = { explaining = false }) { Text(stringResource(R.string.ok)) }
            }
        )
    }
    if (menu != null) {
        val choose = { choice: Int? ->
            feed.input(fields("kind" to "choose", "menu" to menu.number("id"), "choice" to choice))
        }
        val pointed = menu["context"] as? JsonObject
        KodosiDialog(
            onDismissRequest = { choose(null) },
            title = { Text(stringResource(R.string.browser_menu)) },
            text = {
                Column {
                    if (pointed == null) {
                        menu.objects("items").forEachIndexed { index, item ->
                            KodosiTextButton(onClick = { choose(index) }, enabled = item.flag("enabled")) {
                                Text(item.string("label").orEmpty())
                            }
                        }
                    } else {
                        val selection = pointed.string("selection").orEmpty()
                        val link = pointed.string("link")
                        KodosiTextButton(onClick = { choose(0) }, enabled = selection.isNotEmpty()) {
                            Text(stringResource(R.string.copy))
                        }
                        if (link != null) {
                            KodosiTextButton(onClick = { choose(1) }) { Text(stringResource(R.string.copy_link)) }
                        }
                        KodosiTextButton(onClick = {
                            choose(null)
                            val clip = context.getSystemService(ClipboardManager::class.java).primaryClip
                            val words = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)
                            if (!words.isNullOrEmpty()) feed.input(fields("kind" to "text", "text" to words.toString()))
                        }) { Text(stringResource(R.string.paste)) }
                        if (ask != null) {
                            KodosiTextButton(onClick = {
                                choose(null)
                                ask(selection.ifEmpty { link })
                            }) { Text(stringResource(R.string.ask_about_this)) }
                        }
                    }
                }
            },
            confirmButton = {
                KodosiTextButton(onClick = { choose(null) }) { Text(stringResource(R.string.cancel)) }
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
    var window = fields()
        set(value) {
            field = value
            fit()
        }
    var pictureSize: IntSize? = null
        set(value) {
            field = value
            fit()
        }
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
                mouse("wheel", event.x, event.y, dx = dx, dy = dy)
                return true
            }
            override fun onFling(first: MotionEvent?, event: MotionEvent, velocityX: Float, velocityY: Float): Boolean {
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
        // While the detector listens for double taps, it gives the second tap of one to no other callback.
        setOnDoubleTapListener(null)
    }

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    private fun fit() {
        val shown = pictureSize
            ?: IntSize(window.number("width")?.toInt() ?: width, window.number("height")?.toInt() ?: height)
        if (shown.width <= 0 || shown.height <= 0 || width <= 0 || height <= 0) return
        val fit = minOf(width.toFloat() / shown.width, height.toFloat() / shown.height)
        val w = (shown.width * fit).toInt()
        val h = (shown.height * fit).toInt()
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

    private fun countClick(button: String, x: Float, y: Float, pressedAt: Long) {
        val again = button == clickButton && pressedAt - releasedAt <= ViewConfiguration.getDoubleTapTimeout() &&
            hypot(x - clickX, y - clickY) <= doubleTapSlop
        clickCount = if (again) clickCount + 1 else 1
        clickButton = button
        clickX = x
        clickY = y
    }

    private fun mouse(
        action: String,
        x: Float,
        y: Float,
        button: String = "none",
        clicks: Int = 0,
        dx: Float = 0f,
        dy: Float = 0f,
        modifiers: Int = 0
    ): Boolean {
        val bw = window.number("width")?.toFloat() ?: return false
        val bh = window.number("height")?.toFloat() ?: return false
        if (bw <= 0 || bh <= 0) return false
        val fit = minOf(width / bw, height / bh)
        val rawX = (x - (width - bw * fit) / 2) / fit
        val rawY = (y - (height - bh * fit) / 2) / fit
        if ((rawX !in 0f..bw || rawY !in 0f..bh) &&
            action != "release" && !(action == "move" && button != "none")
        ) {
            return false
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
                "clicks" to clicks,
                "deltaX" to (dx / fit),
                "deltaY" to (dy / fit), "modifiers" to modifiers
            )
        )
        return true
    }

    private fun click(x: Float, y: Float, button: String) {
        if (mouse("press", x, y, button, 1)) mouse("release", x, y, button, 1)
    }

    /**
     * A press that rests and then moves drags with the left button, which selects text as a mouse
     * does; a press that rests and lets go opens the menu of the page, as a tap with two fingers does.
     */
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
                    click(pairX, pairY, "right")
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
                    click(holdX, holdY, "right")
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
