package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.ClipboardManager
import android.content.Context
import android.graphics.PointF
import android.os.Handler
import android.os.Looper
import android.view.PixelCopy
import android.view.SurfaceHolder
import android.view.SurfaceView
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
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.PopupProperties
import androidx.core.graphics.createBitmap
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

// The shared browser of the terminal's canvas, which rests on its last picture while its own
// pictures come back, and wakes by itself when someone opens it asleep.
@Composable
fun BrowserScreen(
    model: AppModel,
    session: String,
    browser: CanvasBrowser,
    computer: String,
    offline: Boolean,
    ask: ((String?) -> Unit)? = null
) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    val history by model.preferences.browserHistory.collectAsStateWithLifecycle()
    val feed = remember(session) { model.video.browser(session) }
    val window by feed.window.collectAsStateWithLifecycle()
    val icons by feed.icons.collectAsStateWithLifecycle()
    val picture by feed.picture.collectAsStateWithLifecycle()
    val live by feed.live.collectAsStateWithLifecycle()
    val still by feed.still.collectAsStateWithLifecycle()
    val pictureSize by feed.pictureSize.collectAsStateWithLifecycle()
    val failure by feed.failure.collectAsStateWithLifecycle()
    val undecodable by feed.undecodable.collectAsStateWithLifecycle()
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
    CopiesToClipboard(feed)
    LifecycleResumeEffect(feed) {
        feed.show(true)
        onPauseOrDispose {
            view?.keepStill()
            feed.show(false)
        }
    }
    val waking = browser.state == "asleep" && browser.problem == null && !offline
    LaunchedEffect(waking) {
        if (!waking) return@LaunchedEffect
        try {
            model.canvas.wakeOnce(session)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Exception) {
            // The browser says why it did not wake through its problem.
        }
    }
    val shows = browserShows(
        offline,
        failed = failure != null || undecodable || browser.problem != null,
        getting = browser.getting != null,
        live = live && picture,
        kept = picture || still != null
    )
    val retry = {
        feed.retry()
        if (browser.state == "asleep") scope.launch { model.runAction { model.canvas.wake(session) } }
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
                    Text(stringResource(R.string.shared), style = KodosiType.footnote, color = colors.inkMuted)
                }
            }
            KeyboardButton(view, enabled = awake)
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
                val lastPicture = still
                if (shows == BrowserShows.RESTING) {
                    if (!picture && lastPicture != null) {
                        Image(
                            lastPicture.asImageBitmap(),
                            null,
                            Modifier.fillMaxSize(),
                            contentScale = ContentScale.Fit,
                            colorFilter = remember { ColorFilter.colorMatrix(ColorMatrix(RESTING.array)) }
                        )
                    } else {
                        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.5f)))
                    }
                }
                val fading = fade<Float>()
                AnimatedContent(
                    shows,
                    Modifier.fillMaxSize(),
                    transitionSpec = { (fadeIn(fading) togetherWith fadeOut(fading)).using(null) },
                    contentKey = {
                        it.takeUnless { shown -> shown == BrowserShows.LIVE || shown == BrowserShows.RESTING }
                    },
                    label = "browser state"
                ) { shown ->
                    when (shown) {
                        BrowserShows.OFFLINE -> VideoNote(
                            AppSymbol.OFFLINE,
                            stringResource(R.string.computer_offline, computer)
                        )

                        BrowserShows.CANT -> VideoNote(
                            AppSymbol.WARNING,
                            stringResource(R.string.browser_cant_show),
                            retry
                        )

                        BrowserShows.GETTING -> Column(
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
                        }

                        BrowserShows.OPENING -> FirstWait()

                        BrowserShows.LIVE, BrowserShows.RESTING -> Unit
                    }
                }
                ReconnectingNotice(
                    shows == BrowserShows.RESTING,
                    Modifier.align(Alignment.TopCenter).padding(top = 10.dp),
                    patience = RECONNECTING_PATIENCE
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
            title = { Text(stringResource(R.string.shared)) },
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
// A SurfaceView hands the decoder's pictures straight to the display, a frame sooner than a
// TextureView, which the app draws first.
internal class BrowserSurface(context: Context, private val feed: BrowserFeed) :
    StreamInput(context, feed),
    SurfaceHolder.Callback {
    private val video = SurfaceView(context).also {
        addView(it, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        it.holder.addCallback(this)
    }
    var window = fields()
        set(value) {
            field = value
            place()
        }
    var pictureSize: IntSize? = null
        set(value) {
            field = value
            place()
        }

    private fun place() {
        val shown = pictureSize
            ?: IntSize(window.number("width")?.toInt() ?: width, window.number("height")?.toInt() ?: height)
        if (shown.width <= 0 || shown.height <= 0 || width <= 0 || height <= 0) return
        val fit = minOf(width.toFloat() / shown.width, height.toFloat() / shown.height)
        val w = (shown.width * fit).toInt()
        val h = (shown.height * fit).toInt()
        if (video.layoutParams.width != w || video.layoutParams.height != h) {
            video.layoutParams = LayoutParams(w, h, android.view.Gravity.CENTER)
        }
    }

    private val page: IntSize?
        get() {
            val pageWidth = window.number("width")?.toInt() ?: return null
            val pageHeight = window.number("height")?.toInt() ?: return null
            return IntSize(pageWidth, pageHeight).takeIf { pageWidth > 0 && pageHeight > 0 }
        }

    override fun fit(): Float? = page?.let { minOf(width.toFloat() / it.width, height.toFloat() / it.height) }

    override fun streamPoint(x: Float, y: Float, clamped: Boolean): PointF? {
        val page = page ?: return null
        val fit = fit() ?: return null
        val rawX = (x - (width - page.width * fit) / 2) / fit
        val rawY = (y - (height - page.height * fit) / 2) / fit
        if (!clamped && (rawX !in 0f..page.width.toFloat() || rawY !in 0f..page.height.toFloat())) return null
        return PointF(
            rawX.coerceIn(0f, (page.width - 1).coerceAtLeast(0).toFloat()),
            rawY.coerceIn(0f, (page.height - 1).coerceAtLeast(0).toFloat())
        )
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        place()
        super.onSizeChanged(w, h, oldw, oldh)
    }

    // The browser's surface goes with its view, so the feed keeps a copy of the last picture as the
    // still for the next view, at half size since it shows dimmed and only until the browser's own
    // pictures come.
    fun keepStill() {
        val surface = video.holder.surface
        if (!surface.isValid || !feed.picture.value || video.width < 2 || video.height < 2) return
        val still = createBitmap(video.width / 2, video.height / 2)
        PixelCopy.request(
            video,
            still,
            { copied -> if (copied == PixelCopy.SUCCESS) feed.keep(still) },
            Handler(Looper.getMainLooper())
        )
    }

    override fun surfaceCreated(holder: SurfaceHolder) =
        feed.surface(holder.surface, width, height, resources.displayMetrics.density)
    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
    override fun surfaceDestroyed(holder: SurfaceHolder) {
        keepStill()
        feed.detach()
    }
}
