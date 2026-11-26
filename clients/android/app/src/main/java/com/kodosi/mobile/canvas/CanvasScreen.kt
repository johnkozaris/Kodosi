package com.kodosi.mobile.canvas

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.RuntimeFailure
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.DockItem
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.LinkOpener
import com.kodosi.mobile.ui.LocalLinkOpener
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.SectionDock
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.fade
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun CanvasScreen(model: AppModel, sessionId: String, back: () -> Unit, ask: () -> Unit, initialKey: String? = null) {
    val catalog by model.terminals.state.collectAsStateWithLifecycle()
    val preferences by model.preferences.state.collectAsStateWithLifecycle()
    val session = catalog.sessions.firstOrNull { it.id == sessionId }
    val canvas = session?.canvas
    val context = LocalContext.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val closed = preferences.closedCanvasParts(sessionId, canvas)
    var selected by rememberSaveable(sessionId) {
        mutableStateOf(canvas?.openKey(initialKey, closed) ?: initialKey ?: "browser")
    }
    var part by rememberSaveable(sessionId) { mutableStateOf(canvasPart(selected)) }

    // Remembered across config changes so rotation does not re-claim the shared browser viewport.
    var opening by rememberSaveable(sessionId) { mutableStateOf(true) }
    var shelf by rememberSaveable(sessionId) { mutableStateOf(false) }
    var followedShow by rememberSaveable(sessionId) { mutableLongStateOf(canvas?.open?.at ?: 0L) }
    val shownFile = canvas?.shownFile(selected)
    val item = shownFile?.takeIf { part == "files" && !shelf }
    var menu by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf(false) }
    var actionItem by remember { mutableStateOf<CanvasItem?>(null) }
    val menuItem = actionItem ?: item
    val links = LocalLinkOpener.current
    val closePart: (String) -> Unit = { closing ->
        val next = closed + closing
        model.preferences.canvasPart(sessionId, closing, false, canvas?.partChanges()?.get(closing) ?: 0)
        if (part == closing) {
            val other = canvas?.partChanges()?.keys?.firstOrNull { it != closing && it !in next }
            if (other == null) back() else part = other
        }
    }
    val choose: (CanvasItem) -> Unit = {
        selected = it.key
        part = "files"
        model.preferences.canvasPart(sessionId, "files", true)
        shelf = false
    }
    val actions: (CanvasItem) -> Unit = {
        actionItem = it
        menu = true
    }
    var working by remember { mutableStateOf(false) }
    var browserStarting by remember { mutableStateOf(false) }
    suspend fun wakeBrowser() {
        browserStarting = true
        try {
            model.runAction { model.canvas.wake(sessionId) }
        } finally {
            browserStarting = false
        }
    }
    var saving by remember { mutableStateOf<CanvasItem?>(null) }
    var selection by remember(item?.key) { mutableStateOf<suspend () -> String?>({ null }) }

    fun askAbout(key: String?, words: suspend () -> String?) {
        val terminal = session ?: return
        scope.launch {
            model.runAction {
                val place = writtenOut(key ?: canvas?.browser?.tabs?.firstOrNull { it.active }?.url.orEmpty())
                val line = words()?.replace(Regex("[\\p{Cntrl}\\s]+"), " ")?.trim()?.take(600)
                val prompt = if (line.isNullOrEmpty()) {
                    resources.getString(R.string.canvas_question, place)
                } else {
                    resources.getString(R.string.canvas_question_words, place, line)
                }
                model.terminals.act("session.input", terminal, fields("text" to prompt.trimEnd() + " "))
                ask()
            }
        }
    }
    val export = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
        val target = saving
        saving = null
        if (uri != null && target != null) {
            scope.launch {
                model.runAction {
                    val data = model.canvas.content(sessionId, target)
                    withContext(Dispatchers.IO) {
                        context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(data) }
                            ?: throw RuntimeFailure(model.words.didNotWork)
                    }
                }
            }
        }
    }
    val add = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        scope.launch {
            working = true
            try {
                model.runAction {
                    for (uri in uris) {
                        val file = readPickedFile(context, uri)
                        model.canvas.attach(sessionId, file.first, file.second)?.let {
                            selected = it
                            part = "files"
                            model.preferences.canvasPart(sessionId, "files", true)
                            shelf = false
                        }
                    }
                }
            } finally {
                working = false
            }
        }
    }
    LaunchedEffect(sessionId, initialKey) {
        model.preferences.canvasPart(sessionId, part, true)
        if (opening && part != "files") model.video.choose(sessionId)
        opening = false
        if (initialKey == "browser" && canvas?.browser == null && session?.canControl == true) wakeBrowser()
    }
    LaunchedEffect(part) {
        if (part == "files") model.video.forget(sessionId)
    }
    DisposableEffect(sessionId) { onDispose { model.video.forget(sessionId) } }
    LaunchedEffect(canvas?.open, canvas?.change) {
        session?.let { terminal ->
            model.canvas.newShow(terminal)?.let { show ->
                if (show.at > followedShow) {
                    part = canvasPart(show.key)
                    if (part == "files") selected = show.key
                    model.preferences.canvasPart(sessionId, part, true)
                    shelf = false
                }
            }
            followedShow = canvas?.open?.at ?: followedShow
            model.canvas.saw(terminal)
        }
    }
    // Pin the fallback file in selected so edits to other files do not switch the active view.
    LaunchedEffect(item?.key) { item?.let { selected = it.key } }
    val parts = listOfNotNull(
        if (!canvas?.items.isNullOrEmpty() && "files" !in closed) {
            "files" to DockItem(stringResource(R.string.files), AppSymbol.STACK, canvas.items.size)
        } else {
            null
        },
        if (canvas?.browser != null && "browser" !in closed) {
            "browser" to DockItem(stringResource(R.string.browser), AppSymbol.BROWSER)
        } else {
            null
        },
        if (canvas?.screen != null && "screen" !in closed) {
            "screen" to DockItem(stringResource(R.string.screen), AppSymbol.DISPLAY)
        } else {
            null
        }
    )
    Column(Modifier.fillMaxSize().background(KodosiTheme.colors.sheet)) {
        PageHeader(stringResource(R.string.canvas), back, titleContent = {
            if (parts.isEmpty()) {
                Text(stringResource(R.string.canvas), style = KodosiType.headline)
            } else {
                SectionDock(parts.map { it.second }, parts.indexOfFirst { it.first == part }) {
                    if (parts[it].first != "files") model.video.choose(sessionId)
                    part = parts[it].first
                }
            }
        }, actions = {
            Box {
                ActionIcon(AppSymbol.ADD, R.string.add_to_canvas, { adding = true }, enabled = !working, open = adding)
                KodosiMenu(adding, onDismissRequest = { adding = false }) {
                    if (!canvas?.items.isNullOrEmpty() && "files" in closed) {
                        KodosiMenuItem(text = { Text(stringResource(R.string.files)) }, onClick = {
                            adding = false
                            model.preferences.canvasPart(sessionId, "files", true)
                            part = "files"
                        })
                    }
                    if (canvas?.browser == null || "browser" in closed) {
                        KodosiMenuItem(
                            text = { Text(stringResource(R.string.browser)) },
                            enabled = !browserStarting && (canvas?.browser != null || session?.canControl == true),
                            onClick = {
                                adding = false
                                model.preferences.canvasPart(sessionId, "browser", true)
                                part = "browser"
                                model.video.choose(sessionId)
                                if (canvas?.browser == null) scope.launch { wakeBrowser() }
                            }
                        )
                    }
                    if (session?.showsScreen == true && (canvas?.screen == null || "screen" in closed)) {
                        KodosiMenuItem(
                            text = { Text(stringResource(R.string.screen)) },
                            enabled = canvas?.screen != null || session.canControl,
                            onClick = {
                                adding = false
                                model.preferences.canvasPart(sessionId, "screen", true)
                                part = "screen"
                                model.video.choose(sessionId)
                                if (canvas?.screen == null) {
                                    scope.launch { model.runAction { model.canvas.showScreen(sessionId) } }
                                }
                            }
                        )
                    }
                    KodosiMenuItem(
                        text = { Text(stringResource(R.string.add_file)) },
                        enabled = session?.canControl == true,
                        onClick = {
                            adding = false
                            add.launch(arrayOf("*/*"))
                        }
                    )
                }
            }
            Box {
                ActionIcon(AppSymbol.MORE, R.string.more, {
                    actionItem = null
                    menu = true
                }, open = menu)
                KodosiMenu(menu, onDismissRequest = {
                    menu = false
                    actionItem = null
                }) {
                    if (menuItem != null && !menuItem.missing) {
                        KodosiMenuItem(text = { Text(stringResource(R.string.save_copy)) }, onClick = {
                            menu = false
                            saving = menuItem
                            export.launch(menuItem.name)
                        })
                    }
                    if (session?.canControl == true && session.agent.isAgent &&
                        (menuItem != null || (part == "browser" && canvas?.browser != null))
                    ) {
                        KodosiMenuItem(text = { Text(stringResource(R.string.ask_about_this)) }, onClick = {
                            menu = false
                            val words = if (menuItem == item) selection else ({ null })
                            askAbout(menuItem?.key, words)
                        })
                    }
                    if (menuItem != null) {
                        KodosiMenuItem(text = { Text(stringResource(R.string.remove_from_canvas)) }, onClick = {
                            menu = false
                            scope.launch { model.runAction { model.canvas.remove(sessionId, menuItem.key) } }
                        })
                        if ((canvas?.items?.size ?: 0) > 1) {
                            KodosiMenuItem(text = { Text(stringResource(R.string.remove_all_from_canvas)) }, onClick = {
                                menu = false
                                scope.launch { model.runAction { model.canvas.removeAll(sessionId) } }
                            })
                        }
                        if (!menuItem.missing) {
                            KodosiMenuItem(text = { Text(stringResource(R.string.move_to_trash)) }, onClick = {
                                menu = false
                                scope.launch { model.runAction { model.canvas.trash(sessionId, menuItem.key) } }
                            })
                        }
                    }
                    KodosiMenuItem(text = { Text(stringResource(R.string.close_part)) }, onClick = {
                        menu = false
                        closePart(part)
                    })
                }
            }
        })
        if (part == "files" && !canvas?.items.isNullOrEmpty()) {
            CanvasFilesBar(canvas.items, shownFile?.key.orEmpty(), shelf, { shelf = !shelf }, choose, remove = { file ->
                scope.launch { model.runAction { model.canvas.remove(sessionId, file.key) } }
            }, actions = actions)
        }
        Unfolding(working) { ProgressCaption(stringResource(R.string.uploading)) }
        Unfolding(browserStarting) { ProgressCaption(stringResource(R.string.loading)) }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            CompositionLocalProvider(
                LocalLinkOpener provides LinkOpener { address, computer ->
                    links.open(address, computer ?: session?.hostName)
                }
            ) {
                when {
                    part == "files" && shelf -> CanvasShelf(
                        canvas?.items.orEmpty(),
                        model.canvas,
                        sessionId,
                        choose,
                        actions
                    )

                    item?.missing == true -> EmptyState(
                        item.name,
                        stringResource(R.string.file_unavailable),
                        AppSymbol.FILE
                    )

                    item != null && item.view != null && item.view !in preferences.disabledViewers ->
                        key(item.key) {
                            DocumentView(model, sessionId, item, Modifier.fillMaxSize(), selection = {
                                selection =
                                    it
                            })
                        }

                    item != null -> key(item.key) {
                        CanvasFile(model, sessionId, item, Modifier.fillMaxSize(), selection = {
                            selection = it
                        })
                    }

                    part == "browser" && canvas?.browser != null -> BrowserScreen(
                        model,
                        sessionId,
                        canvas.browser,
                        ask = if (session.canControl && session.agent.isAgent) {
                            { words -> askAbout(null) { words } }
                        } else {
                            null
                        }
                    )

                    part == "screen" && canvas?.screen != null -> ScreenView(
                        model,
                        sessionId,
                        session.hostName ?: stringResource(R.string.remote_computer)
                    )

                    else -> EmptyState(stringResource(R.string.canvas_empty), symbol = AppSymbol.CANVAS) {
                        if (session?.canControl == true) {
                            KodosiTextButton(onClick = {
                                adding = true
                            }) { Text(stringResource(R.string.add_to_canvas)) }
                        }
                    }
                }
            }
        }
    }
}

// Keep showing the previous revision until the new bytes load so live edits do not reset scroll.
@Composable
private fun CanvasFile(
    model: AppModel,
    session: String,
    item: CanvasItem,
    modifier: Modifier,
    selection: (suspend () -> String?) -> Unit
) {
    var content by remember { mutableStateOf<ByteArray?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableIntStateOf(0) }
    LaunchedEffect(item.revision, retry) {
        try {
            content = model.canvas.content(session, item)
            failure = null
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            content = null
            failure = error.localizedMessage ?: model.words.didNotWork
        }
    }
    val bytes = content
    when {
        bytes != null && item.kind == "page" -> PreviewPage(bytes, modifier, selection)

        bytes != null && item.kind == "picture" -> Picture(bytes, item.name, modifier)

        bytes != null -> NativeFile(bytes, item.name, item.kind, modifier)

        failure != null -> Column(Modifier.padding(24.dp)) {
            ErrorNote(failure.orEmpty())
            KodosiTextButton(onClick = {
                failure = null
                retry++
            }) { Text(stringResource(R.string.try_again)) }
        }

        else -> Box(modifier, contentAlignment = Alignment.Center) { ProgressCaption(stringResource(R.string.loading)) }
    }
}

@Composable
private fun Picture(bytes: ByteArray, name: String, modifier: Modifier) {
    var decoded by remember { mutableStateOf<Result<Bitmap>?>(null) }
    var zoom by rememberSaveable { mutableFloatStateOf(1f) }
    var pan by remember { mutableStateOf(Offset.Zero) }
    LaunchedEffect(bytes) {
        decoded = withContext(Dispatchers.Default) {
            runCatching {
                ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
                    val sample = maxOf(1, maxOf(info.size.width, info.size.height) / 4096)
                    decoder.setTargetSampleSize(sample)
                }
            }
        }
    }
    Crossfade(decoded, modifier, fade(), label = "picture") { result ->
        val picture = result?.getOrNull()
        if (picture != null) {
            Box(
                Modifier.fillMaxSize().pointerInput(Unit) {
                    detectTransformGestures { _, move, scale, _ ->
                        zoom =
                            (zoom * scale).coerceIn(1f, 6f)
                        pan = if (zoom == 1f) Offset.Zero else pan + move
                    }
                }
            ) {
                Image(
                    picture.asImageBitmap(),
                    name,
                    Modifier.fillMaxSize().graphicsLayer(
                        scaleX = zoom,
                        scaleY = zoom,
                        translationX = pan.x,
                        translationY = pan.y
                    )
                )
            }
        } else if (result != null) {
            EmptyState(name, stringResource(R.string.file_unavailable), AppSymbol.FILE)
        }
    }
}

internal suspend fun readPickedFile(context: android.content.Context, uri: Uri): Pair<String, ByteArray> =
    withContext(Dispatchers.IO) {
        val name =
            context.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE),
                null,
                null,
                null
            )?.use { cursor ->
                if (!cursor.moveToFirst()) {
                    null
                } else {
                    if (!cursor.isNull(1) &&
                        cursor.getLong(1) > CanvasStore.MAX_FILE_BYTES
                    ) {
                        throw RuntimeFailure(context.getString(R.string.file_too_large))
                    }
                    cursor.getString(0)
                }
            } ?: "file"
        val data = context.contentResolver.openInputStream(uri)?.use { input ->
            val chunks = mutableListOf<ByteArray>()
            var total = 0
            while (total <= CanvasStore.MAX_FILE_BYTES) {
                val chunk = ByteArray(minOf(8192, CanvasStore.MAX_FILE_BYTES + 1 - total))
                val count = input.read(chunk)
                if (count < 0) break
                val received = if (count == 0) {
                    val single = input.read()
                    if (single < 0) break
                    byteArrayOf(single.toByte())
                } else if (count == chunk.size) {
                    chunk
                } else {
                    chunk.copyOf(count)
                }
                chunks += received
                total += received.size
            }
            if (total > CanvasStore.MAX_FILE_BYTES) throw RuntimeFailure(context.getString(R.string.file_too_large))
            ByteArray(total).also { result ->
                var offset = 0
                for (chunk in chunks) {
                    chunk.copyInto(result, offset)
                    offset += chunk.size
                }
            }
        }
            ?: throw RuntimeFailure(context.getString(R.string.file_unavailable))
        name to data
    }

/** A name of a file can hold a line break, which would end the line that the agent reads. */
private fun writtenOut(text: String): String = buildString {
    text.codePoints().forEach { point ->
        val breaks = Character.isISOControl(point) || Character.getType(point) == Character.FORMAT.toInt() ||
            point == 0x2028 || point == 0x2029
        if (breaks) append("\\u{%X}".format(point)) else appendCodePoint(point)
    }
}
