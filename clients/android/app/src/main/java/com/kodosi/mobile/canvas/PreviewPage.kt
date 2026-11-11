package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.view.ViewGroup
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import androidx.core.graphics.createBitmap
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.LocalLinkOpener
import java.io.ByteArrayInputStream
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@SuppressLint("SetJavaScriptEnabled")
@Composable
fun PreviewPage(content: ByteArray, modifier: Modifier, selection: (suspend () -> String?) -> Unit) {
    val context = LocalContext.current
    val links = LocalLinkOpener.current
    var retry by remember { mutableIntStateOf(0) }
    var failed by remember { mutableStateOf<WebView?>(null) }
    val web = remember(retry) {
        WebView(context).apply {
            settings.javaScriptEnabled = true
            settings.allowFileAccess = false
            settings.allowContentAccess = false
            settings.domStorageEnabled = false
            settings.mixedContentMode = android.webkit.WebSettings.MIXED_CONTENT_NEVER_ALLOW
        }
    }
    DisposableEffect(web) {
        selection {
            if (failed === web) {
                null
            } else {
                kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                    web.evaluateJavascript("getSelection().toString().slice(0,2000)") {
                        if (continuation.isActive) {
                            continuation.resumeWith(
                                Result.success(
                                    runCatching {
                                        RuntimeJson.decodeFromString<String>(it)
                                    }.getOrNull()
                                )
                            )
                        }
                    }
                }
            }
        }
        onDispose {
            selection { null }
            if (failed !== web) web.destroy()
        }
    }
    LaunchedEffect(content, web) {
        failed = null
        web.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                if (request.url.toString() != PREVIEW) return denied()
                return WebResourceResponse(
                    "text/html",
                    "UTF-8",
                    200,
                    "OK",
                    mapOf(
                        "Content-Security-Policy" to POLICY,
                        "Cache-Control" to "no-store"
                    ),
                    ByteArrayInputStream(content)
                )
            }

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                if (request.url.toString() == PREVIEW) return false
                if (request.isForMainFrame && request.hasGesture()) links.open(request.url.toString())
                return true
            }

            override fun onRenderProcessGone(view: WebView, detail: android.webkit.RenderProcessGoneDetail): Boolean {
                failed = view
                (view.parent as? ViewGroup)?.removeView(view)
                view.destroy()
                return true
            }
        }
        web.loadUrl(PREVIEW)
    }
    if (failed === web) {
        EmptyState(stringResource(R.string.file_unavailable)) {
            KodosiTextButton(onClick = { retry++ }) { Text(stringResource(R.string.try_again)) }
        }
    } else {
        AndroidView(factory = { web }, modifier = modifier)
    }
}

@Composable
fun NativeFile(content: ByteArray, name: String, kind: String, modifier: Modifier) {
    val context = LocalContext.current
    var file by remember(content) { mutableStateOf<File?>(null) }
    LaunchedEffect(content) {
        file = withContext(Dispatchers.IO) {
            File.createTempFile(
                "canvas-",
                "." + name.substringAfterLast('.', "bin"),
                File(context.cacheDir, "canvas").apply {
                    mkdirs()
                }
            ).apply { writeBytes(content) }
        }
    }
    DisposableEffect(file) {
        val current = file
        onDispose { current?.delete() }
    }
    val current = file ?: return
    if (kind == "pdf") {
        PdfPages(current, modifier)
    } else {
        EmptyState(name, symbol = com.kodosi.mobile.ui.AppSymbol.FILE) {
            KodosiTextButton(onClick = {
                val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", current)
                val type =
                    android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(
                        name.substringAfterLast('.').lowercase()
                    )
                        ?: "application/octet-stream"
                runCatching {
                    context.startActivity(
                        Intent.createChooser(
                            Intent(
                                Intent.ACTION_VIEW
                            ).setDataAndType(uri, type).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
                            null
                        )
                    )
                }
            }) { Text(stringResource(R.string.open_in_app)) }
        }
    }
}

@Composable
private fun PdfPages(file: File, modifier: Modifier) {
    var page by remember(file) { mutableIntStateOf(0) }
    var count by remember(file) { mutableIntStateOf(0) }
    var bitmap by remember(file) { mutableStateOf<Bitmap?>(null) }
    var failed by remember(file) { mutableStateOf(false) }
    LaunchedEffect(file, page) {
        failed = false
        val rendered = withContext(Dispatchers.IO) {
            runCatching {
                PdfRenderer(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)).use { renderer ->
                    renderer.openPage(page.coerceIn(0, renderer.pageCount - 1)).use { opened ->
                        val scale = minOf(2f, 3000f / maxOf(opened.width, opened.height))
                        createBitmap(
                            (opened.width * scale).toInt(),
                            (opened.height * scale).toInt()
                        ).also {
                            it.eraseColor(android.graphics.Color.WHITE)
                            opened.render(it, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                        } to renderer.pageCount
                    }
                }
            }.getOrNull()
        }
        bitmap = rendered?.first
        count = rendered?.second ?: 0
        failed = rendered == null
    }
    Column(modifier) {
        Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
            bitmap?.let { Image(it.asImageBitmap(), null, Modifier.fillMaxSize()) }
            if (failed) EmptyState(stringResource(R.string.file_unavailable))
        }
        Row(Modifier.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            KodosiTextButton(onClick = { page-- }, enabled = page > 0) { Text(stringResource(R.string.previous_page)) }
            Text("${if (count == 0) 0 else page + 1} / $count", Modifier.weight(1f))
            KodosiTextButton(onClick = {
                page++
            }, enabled = page + 1 < count) { Text(stringResource(R.string.next_page)) }
        }
    }
}

private const val PREVIEW = "https://canvas.kodosi.invalid/item"
private const val POLICY =
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; " +
        "font-src data:; media-src data: blob:; form-action 'none'; base-uri 'none'; frame-src 'none'"
