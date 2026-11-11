package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.Context
import android.content.MutableContextWrapper
import android.view.ViewGroup
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import com.kodosi.mobile.AppLanguage
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiColors
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.LocalLinkOpener
import java.io.ByteArrayInputStream
import java.util.Base64
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

@Composable
fun DocumentView(
    model: AppModel,
    session: String,
    item: CanvasItem,
    modifier: Modifier = Modifier,
    selection: (suspend () -> String?) -> Unit = {}
) {
    val context = LocalContext.current
    val preferences by model.preferences.state.collectAsStateWithLifecycle()
    val colors = KodosiTheme.colors
    val links = LocalLinkOpener.current
    val page = remember(session, item.key, item.view) {
        model.documents.acquire(context, session, item, links::open)
    }
    DisposableEffect(page) {
        selection { page.selected() }
        onDispose {
            model.documents.release(page)
            selection { null }
        }
    }
    LifecycleResumeEffect(page) {
        page.resume()
        onPauseOrDispose { page.pause() }
    }
    if (page.stopped || page.problem != null) {
        Column(Modifier.padding(24.dp)) {
            ErrorNote(stringResource(page.problem ?: R.string.document_stopped))
            KodosiTextButton(onClick = page::retry) { Text(stringResource(R.string.try_again)) }
        }
    } else {
        key(page.web) {
            AndroidView(
                factory = { page.web },
                modifier = modifier,
                update = {
                    page.look(colors, fields("pages" to preferences.documentPages, "dark" to preferences.documentDark))
                }
            )
        }
    }
}

internal class DocumentPages(
    private val context: Context,
    private val canvas: CanvasStore,
    private val scope: CoroutineScope,
    private val failure: (String) -> Unit
) {
    private data class Key(val session: String, val item: String, val kind: String)
    private data class Entry(val page: DocumentPage, var expiry: Job? = null, var visible: Boolean = true)
    private val pages = LinkedHashMap<Key, Entry>(4, 0.75f, true)

    fun acquire(context: Context, session: String, item: CanvasItem, openLink: (String) -> Unit): DocumentPage {
        val key = Key(session, item.key, item.view ?: "text")
        val entry = pages.getOrPut(key) {
            Entry(DocumentPage(this.context, scope, canvas.link(session, item.key), key.kind, failure))
        }
        entry.expiry?.cancel()
        entry.visible = true
        entry.page.bind(context, openLink)
        while (pages.size > 4) {
            val oldest = pages.entries.firstOrNull { !it.value.visible } ?: break
            oldest.value.expiry?.cancel()
            oldest.value.page.close()
            pages.remove(oldest.key)
        }
        return entry.page
    }

    fun release(page: DocumentPage) {
        val entry = pages.values.firstOrNull { it.page === page } ?: return
        entry.visible = false
        page.pause()
        page.unbind()
        entry.expiry = scope.launch {
            delay(5 * 60_000L)
            pages.entries.removeAll { it.value === entry }
            page.close()
        }
    }

    fun reset() {
        pages.values.forEach {
            it.expiry?.cancel()
            it.page.close()
        }
        pages.clear()
    }
}

@SuppressLint("SetJavaScriptEnabled")
internal class DocumentPage(
    context: Context,
    private val scope: CoroutineScope,
    private val link: DocumentLink,
    private val kind: String,
    private val failure: (String) -> Unit
) {
    private val context = MutableContextWrapper(context.applicationContext)
    private var openLink: (String) -> Unit = {}
    private var ready = false
    private var live = false
    private var closed = false
    private var joining: Job? = null
    private var stable: Job? = null
    private var crashes = 0
    var stopped by mutableStateOf(false)
        private set
    var problem by mutableStateOf<Int?>(null)
        private set
    private var theme: JsonObject = fields()
    private var preferences: JsonObject = fields()
    private val loader = WebViewAssetLoader.Builder().addPathHandler(
        "/assets/",
        WebViewAssetLoader.AssetsPathHandler(context)
    ).build()
    var web by mutableStateOf(createWeb())
        private set

    init {
        link.onFrame = { frame ->
            if (ready && !closed) {
                val encoded = JsonPrimitive(Base64.getEncoder().encodeToString(frame))
                web.evaluateJavascript("documentHost.deliver($encoded)", null)
            }
        }
        link.onLost = {
            if (ready) web.evaluateJavascript("documentHost.detach()", null)
            join()
        }
        web.loadUrl(START)
    }

    private fun createWeb(): WebView = WebView(context).also { web ->
        web.layoutParams =
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        web.setBackgroundColor(Color.Transparent.toArgb())
        web.settings.apply {
            javaScriptEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            domStorageEnabled = false
            databaseEnabled = false
            mixedContentMode = android.webkit.WebSettings.MIXED_CONTENT_NEVER_ALLOW
            setSupportMultipleWindows(false)
        }
        web.webChromeClient = object : WebChromeClient() {
            override fun onPermissionRequest(request: PermissionRequest) = request.deny()
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(web, "documentNative", setOf(ORIGIN)) { _, message, origin, main, _ ->
                if (!main || origin.toString() != ORIGIN || closed) return@addWebMessageListener
                val value =
                    runCatching {
                        com.kodosi.mobile.runtime.RuntimeJson.parseToJsonElement(message.data.orEmpty()) as? JsonObject
                    }.getOrNull()
                        ?: return@addWebMessageListener
                when (value.string("type")) {
                    "frame" -> value.string("data")?.takeIf { it.length <= 90_000_000 }?.let {
                        runCatching { Base64.getDecoder().decode(it) }.getOrNull()?.let(link::send)
                    }
                }
            }
        } else {
            problem = R.string.webview_update
        }
        web.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                val response = loader.shouldInterceptRequest(request.url) ?: denied()
                response.responseHeaders = mapOf("Content-Security-Policy" to POLICY, "Cache-Control" to "no-store")
                return response
            }

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                if (request.url.toString() == START && !ready) return false
                if (request.isForMainFrame && request.hasGesture()) scope.launch { openLink(request.url.toString()) }
                return true
            }

            override fun onPageFinished(view: WebView, url: String) {
                if (url != START || closed || view !== this@DocumentPage.web || problem != null || ready) return
                val language = JsonPrimitive(AppLanguage.tag(context))
                view.evaluateJavascript(
                    """
                    (() => {
                      try {
                        window.documentHost = window.documentView.hosted(
                          {kind: ${JsonPrimitive(kind)}, theme: $theme, preferences: $preferences, language: $language},
                          frame => documentNative.postMessage(JSON.stringify({type:"frame", data:frame}))
                        );
                        return !!window.documentHost;
                      } catch (_) { return false; }
                    })()
                    """.trimIndent()
                ) { initialized ->
                    if (closed || view !== this@DocumentPage.web || stopped) return@evaluateJavascript
                    if (initialized != "true") {
                        problem = R.string.file_unavailable
                        return@evaluateJavascript
                    }
                    ready = true
                    stable?.cancel()
                    stable = scope.launch {
                        delay(60_000L)
                        crashes = 0
                    }
                    join()
                }
            }

            override fun onReceivedError(
                view: WebView,
                request: WebResourceRequest,
                error: android.webkit.WebResourceError
            ) {
                if (view === this@DocumentPage.web && request.isForMainFrame && !closed) {
                    problem = R.string.file_unavailable
                }
            }

            override fun onRenderProcessGone(view: WebView, detail: android.webkit.RenderProcessGoneDetail): Boolean {
                if (closed || stopped || view !== this@DocumentPage.web) return true
                ready = false
                link.detach()
                joining?.cancel()
                stable?.cancel()
                (view.parent as? ViewGroup)?.removeView(view)
                view.destroy()
                if (!closed) {
                    crashes++
                    stopped = crashes > 2
                    if (!stopped) {
                        problem = null
                        this@DocumentPage.web = createWeb()
                        this@DocumentPage.web.loadUrl(START)
                        failure(context.getString(R.string.document_reopened))
                    }
                }
                return true
            }
        }
    }

    fun bind(context: Context, openLink: (String) -> Unit) {
        this.context.baseContext = context
        this.openLink = openLink
    }

    fun unbind() {
        context.baseContext = context.applicationContext
        openLink = {}
        (web.parent as? ViewGroup)?.removeView(web)
    }

    fun look(colors: KodosiColors, preferences: JsonObject) {
        fun Color.hex() = "#%06X".format(toArgb() and 0xffffff)
        val next = fields(
            "surface" to colors.surface.hex(), "surface-behind" to colors.well.hex(), "raised" to colors.raised.hex(),
            "lifted" to colors.lifted.hex(), "well" to colors.well.hex(), "ink" to colors.ink.hex(),
            "ink-soft" to colors.inkMuted.hex(), "ink-faint" to colors.inkFaint.hex(), "line" to colors.hairline.hex(),
            "accent" to colors.accentStrong.hex(), "scheme" to if (colors.isDark) "dark" else "light"
        )
        if (ready && next != theme) web.evaluateJavascript("documentHost.restyle($next)", null)
        if (ready && this.preferences != preferences) web.evaluateJavascript("documentHost.prefer($preferences)", null)
        theme = next
        this.preferences = preferences
    }

    fun resume() {
        if (closed) return
        live = true
        if (!stopped) web.onResume()
        join()
    }

    fun pause() {
        if (closed) return
        live = false
        joining?.cancel()
        link.detach()
        if (ready) web.evaluateJavascript("documentHost.detach()", null)
        if (!stopped) web.onPause()
    }

    fun retry() {
        if (closed || (!stopped && problem == null)) return
        joining?.cancel()
        stable?.cancel()
        link.detach()
        ready = false
        if (!stopped) {
            (web.parent as? ViewGroup)?.removeView(web)
            web.destroy()
        }
        crashes = 0
        stopped = false
        problem = null
        web = createWeb()
        web.loadUrl(START)
        if (!live) web.onPause()
    }

    private fun join() {
        if (!live || !ready || closed) return
        joining?.cancel()
        joining = scope.launch {
            var wait = 1000L
            while (live && !closed) {
                try {
                    link.open()
                    if (live && !closed) web.evaluateJavascript("documentHost.attach()", null)
                    return@launch
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (_: Exception) {
                    delay(wait)
                    wait = minOf(wait * 2, 30_000)
                }
            }
        }
    }

    suspend fun selected(): String? = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
        if (!ready || closed) {
            continuation.resumeWith(Result.success(null))
        } else {
            web.evaluateJavascript("documentHost.selection()") { answer ->
                if (continuation.isActive) {
                    val value = runCatching {
                        com.kodosi.mobile.runtime.RuntimeJson.decodeFromString<String?>(answer)
                    }.getOrNull()
                    continuation.resumeWith(Result.success(value?.takeIf { it.isNotBlank() }))
                }
            }
        }
    }

    fun close() {
        if (closed) return
        closed = true
        stable?.cancel()
        joining?.cancel()
        link.close()
        if (!stopped) {
            web.stopLoading()
            web.destroy()
        }
    }

    private companion object {
        const val ORIGIN = "https://appassets.androidplatform.net"
        const val START = "$ORIGIN/assets/documents/index.html"
        const val POLICY =
            "default-src 'none'; script-src 'self'; worker-src 'self'; style-src 'self' 'unsafe-inline'; " +
                "img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; form-action 'none'; base-uri 'none'; frame-src 'none'"
    }
}

internal fun denied() =
    WebResourceResponse("text/plain", "UTF-8", 403, "Blocked", emptyMap(), ByteArrayInputStream(byteArrayOf()))
