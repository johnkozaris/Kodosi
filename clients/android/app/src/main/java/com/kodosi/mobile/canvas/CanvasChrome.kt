package com.kodosi.mobile.canvas

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.text.Html
import androidx.annotation.StringRes
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.graphics.createBitmap
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.FileIcon
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.reducedMotion
import java.io.File
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun CanvasTab(
    title: String,
    selected: Boolean,
    choose: () -> Unit,
    icon: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    close: (() -> Unit)? = null,
    @StringRes closeLabel: Int = R.string.close,
    actions: (() -> Unit)? = null
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(Radius.xs)
    Box(
        modifier.widthIn(min = 80.dp, max = 220.dp)
            .defaultMinSize(minHeight = 48.dp)
            .combinedClickable(
                interactionSource = interaction,
                indication = null,
                role = Role.Tab,
                onClick = choose,
                onLongClick = actions
            ).semantics { this.selected = selected }
    ) {
        Box(
            Modifier.matchParentSize().padding(vertical = 5.dp).then(
                if (selected) {
                    Modifier.raised(colors, shape)
                } else {
                    Modifier.background(
                        if (hovered || focused) colors.ink.copy(alpha = 0.06f) else Color.Transparent,
                        shape
                    )
                }
            )
        )
        Row(
            Modifier.defaultMinSize(minHeight = 48.dp).padding(start = 7.dp, end = 5.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            icon()
            Text(
                title,
                Modifier.weight(1f, fill = false),
                style = KodosiType.footnote.copy(fontWeight = FontWeight.Medium),
                color = if (selected) colors.ink else colors.inkMuted,
                maxLines = 1,
                overflow = TextOverflow.MiddleEllipsis
            )
            close?.let { RemoveKey(closeLabel, it) }
        }
    }
}

@Composable
private fun RemoveKey(@StringRes label: Int, close: () -> Unit) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    Box(
        Modifier.size(48.dp).clickable(interaction, indication = null, role = Role.Button, onClick = close),
        contentAlignment = Alignment.Center
    ) {
        Box(
            Modifier.size(22.dp)
                .background(colors.ink.copy(alpha = if (hovered || focused) 0.14f else 0.08f), CircleShape),
            contentAlignment = Alignment.Center
        ) {
            AppIcon(AppSymbol.CLOSE, Modifier.size(11.dp), description = stringResource(label))
        }
    }
}

@Composable
internal fun CanvasFilesBar(
    files: List<CanvasItem>,
    selected: String,
    shelf: Boolean,
    showShelf: () -> Unit,
    choose: (CanvasItem) -> Unit,
    remove: (CanvasItem) -> Unit,
    actions: (CanvasItem) -> Unit
) {
    val colors = KodosiTheme.colors
    val list = rememberLazyListState()
    LaunchedEffect(selected, files) {
        val index = files.indexOfFirst { it.key == selected }
        if (index >= 0) list.scrollToItem(index)
    }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 4.dp).selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        val label = stringResource(if (shelf) R.string.tabs else R.string.cards)
        Box(
            Modifier.defaultMinSize(minWidth = 48.dp, minHeight = 48.dp)
                .selectable(
                    shelf,
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    role = Role.Tab,
                    onClick = showShelf
                )
                .semantics { contentDescription = label },
            contentAlignment = Alignment.Center
        ) {
            Row(
                Modifier.heightIn(min = 38.dp)
                    .then(if (shelf) Modifier.raised(colors, RoundedCornerShape(Radius.xs)) else Modifier)
                    .padding(horizontal = 9.dp),
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                AppIcon(AppSymbol.STACK, Modifier.size(12.dp))
                Text(
                    files.size.toString(),
                    style = KodosiType.footnote,
                    color = if (shelf) colors.ink else colors.inkMuted
                )
            }
        }
        LazyRow(Modifier.weight(1f), state = list, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            itemsIndexed(files, key = { _, file -> file.key }) { _, file ->
                CanvasTab(
                    file.name,
                    !shelf && selected == file.key,
                    choose = { choose(file) },
                    icon = { FileIcon(file.name, file.kind, 14.dp, file.missing) },
                    close = { remove(file) },
                    closeLabel = R.string.remove_from_canvas,
                    actions = { actions(file) }
                )
            }
        }
    }
}

@Composable
internal fun CanvasShelf(
    files: List<CanvasItem>,
    store: CanvasStore,
    session: String,
    choose: (CanvasItem) -> Unit,
    actions: (CanvasItem) -> Unit
) {
    LazyVerticalGrid(
        GridCells.Adaptive(104.dp),
        Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 18.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        items(files.asReversed(), key = { it.key }) { file ->
            CanvasCard(file, store.stored(session, file), { choose(file) }, { actions(file) })
        }
    }
}

@Composable
private fun CanvasCard(file: CanvasItem, content: ByteArray?, choose: () -> Unit, actions: () -> Unit) {
    val colors = KodosiTheme.colors
    val context = LocalContext.current
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val lift by animateFloatAsState(
        if (hovered || focused) 1f else 0f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 420f),
        label = "file lift"
    )
    var preview by remember(file.key) { mutableStateOf<ShelfPreview?>(null) }
    LaunchedEffect(file.revision, content) {
        preview = content?.let { withContext(Dispatchers.IO) { shelfPreview(it, file.kind, context.cacheDir) } }
    }
    Column(
        Modifier.wrapContentWidth().widthIn(max = 120.dp).combinedClickable(
            interaction,
            indication = null,
            role = Role.Button,
            onClick = choose,
            onLongClick = actions
        ),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        val shape = RoundedCornerShape(Radius.md)
        Box(
            Modifier.fillMaxWidth().height(76.dp).graphicsLayer {
                translationY = -2.dp.toPx() * lift
                rotationZ = -1.2f * lift
            }.then(
                if (file.missing) {
                    Modifier.drawWithContent {
                        drawContent()
                        drawRoundRect(
                            colors.hairline,
                            cornerRadius = CornerRadius(Radius.md.toPx()),
                            style = Stroke(
                                1.5.dp.toPx(),
                                pathEffect = PathEffect.dashPathEffect(
                                    floatArrayOf(5.dp.toPx(), 4.dp.toPx())
                                )
                            )
                        )
                    }
                } else {
                    Modifier.raised(
                        colors,
                        shape,
                        fill = colors.lifted,
                        elevation = if (hovered || focused) Elevation.LIFTED else Elevation.RESTING
                    )
                }
            ).clip(shape),
            contentAlignment = Alignment.Center
        ) {
            when (val shown = preview) {
                is ShelfPreview.Picture -> Image(
                    shown.bitmap.asImageBitmap(),
                    null,
                    Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop
                )

                is ShelfPreview.Title -> Column(
                    Modifier.fillMaxSize().padding(9.dp),
                    verticalArrangement = Arrangement.spacedBy(5.dp)
                ) {
                    Text(
                        shown.title,
                        style = KodosiType.caption.copy(fontWeight = FontWeight.Bold),
                        color = colors.ink,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis
                    )
                    listOf(0.92f, 0.8f, 0.86f, 0.5f).forEach {
                        Box(
                            Modifier.fillMaxWidth(it).height(3.dp).background(
                                colors.inkFaint.copy(alpha = 0.35f),
                                CircleShape
                            )
                        )
                    }
                }

                null -> FileIcon(file.name, file.kind, missing = file.missing)
            }
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                file.name,
                style = KodosiType.footnote.copy(fontWeight = FontWeight.Medium),
                color = if (file.missing) colors.inkFaint else colors.ink,
                maxLines = 1,
                textAlign = TextAlign.Center,
                overflow = TextOverflow.MiddleEllipsis
            )
            val size = android.text.format.Formatter.formatShortFileSize(context, file.bytes)
            val ending = file.name.substringAfterLast('.', "").uppercase()
            Text(
                if (file.missing) {
                    stringResource(R.string.file_unavailable)
                } else if (ending.isEmpty()) {
                    size
                } else {
                    "$ending \u00b7 $size"
                },
                style = KodosiType.caption,
                color = colors.inkFaint,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}

private sealed interface ShelfPreview {
    data class Picture(val bitmap: Bitmap) : ShelfPreview
    data class Title(val title: String) : ShelfPreview
}

private fun shelfPreview(content: ByteArray, kind: String, directory: File): ShelfPreview? = runCatching {
    when (kind) {
        "picture" -> ShelfPreview.Picture(
            ImageDecoder.decodeBitmap(
                ImageDecoder.createSource(ByteBuffer.wrap(content))
            ) { decoder, info, _ ->
                val scale = 360f / maxOf(info.size.width, info.size.height).coerceAtLeast(360)
                decoder.setTargetSize(
                    (info.size.width * scale).toInt().coerceAtLeast(1),
                    (info.size.height * scale).toInt().coerceAtLeast(1)
                )
            }
        )

        "pdf" -> {
            val file = File.createTempFile("canvas-preview-", ".pdf", directory)
            try {
                file.writeBytes(content)
                ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
                    PdfRenderer(descriptor).use { pdf ->
                        if (pdf.pageCount == 0) {
                            null
                        } else {
                            pdf.openPage(0).use { page ->
                                val scale = 360f / maxOf(page.width, page.height).coerceAtLeast(1)
                                val bitmap = createBitmap(
                                    (page.width * scale).toInt().coerceAtLeast(1),
                                    (page.height * scale).toInt().coerceAtLeast(1)
                                )
                                bitmap.eraseColor(android.graphics.Color.WHITE)
                                page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                                ShelfPreview.Picture(bitmap)
                            }
                        }
                    }
                }
            } finally {
                file.delete()
            }
        }

        "page" -> {
            val head = content.decodeToString(endIndex = minOf(content.size, 64 * 1024))
            listOf("title", "h1").firstNotNullOfOrNull { tag ->
                Regex("<$tag\\b[^>]*>(.*?)</$tag>", setOf(RegexOption.IGNORE_CASE, RegexOption.DOT_MATCHES_ALL))
                    .find(head)?.groupValues?.get(1)?.let {
                        Html.fromHtml(it, Html.FROM_HTML_MODE_LEGACY).toString().replace(Regex("\\s+"), " ")
                            .trim().take(240).takeIf(String::isNotEmpty)
                    }
            }?.let { ShelfPreview.Title(it) }
        }

        else -> null
    }
}.getOrNull()
