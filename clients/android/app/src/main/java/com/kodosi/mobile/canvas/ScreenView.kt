package com.kodosi.mobile.canvas

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.DarkColors
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.ReconnectingNotice
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.well
import kotlin.math.roundToInt

private enum class ScreenCover { UNRECORDED, UNDECODABLE, FAILED, WAITING }

// The screen of the terminal's computer, which its owner watches and controls from this device.
@Composable
fun ScreenView(model: AppModel, session: String, computer: String) {
    val scope = rememberCoroutineScope()
    val feed = remember(session) { ScreenFeed(model.video, session, scope, VideoCodec.decodable) }
    val picture by feed.picture.collectAsStateWithLifecycle()
    val pictureSize by feed.pictureSize.collectAsStateWithLifecycle()
    val region by feed.region.collectAsStateWithLifecycle()
    val reconnecting by feed.reconnecting.collectAsStateWithLifecycle()
    val failure by feed.failure.collectAsStateWithLifecycle()
    val undecodable by feed.undecodable.collectAsStateWithLifecycle()
    val access by feed.access.collectAsStateWithLifecycle()
    val zoom by feed.zoom.collectAsStateWithLifecycle()
    val placing by feed.placing.collectAsStateWithLifecycle()
    var view by remember { mutableStateOf<ScreenSurface?>(null) }
    var area by remember { mutableStateOf(IntSize.Zero) }
    var explaining by remember { mutableStateOf(false) }
    DisposableEffect(feed) { onDispose { feed.close() } }
    LifecycleResumeEffect(feed) {
        feed.show(true)
        onPauseOrDispose { feed.show(false) }
    }
    val controls = access?.input != false
    val placement = remember(zoom, placing, area) { feed.placement(area.width.toFloat(), area.height.toFloat()) }
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            ScreenSources(feed, Modifier.weight(1f, fill = false))
            if (zoom > 1f) ActionIcon(AppSymbol.SHRINK, R.string.screen_whole, feed::showWhole)
            if (!controls) {
                Text(
                    stringResource(R.string.screen_view_only),
                    Modifier.clip(CircleShape).background(KodosiTheme.colors.caution.copy(alpha = 0.12f))
                        .clickable(role = Role.Button) { explaining = true }
                        .heightIn(min = 36.dp).padding(horizontal = 10.dp, vertical = 9.dp),
                    style = KodosiType.footnote,
                    color = KodosiTheme.colors.caution
                )
            }
            Box(Modifier.weight(1f))
            ActionIcon(AppSymbol.KEYBOARD, R.string.keyboard, { view?.keyboard() }, enabled = picture && controls)
        }
        Box(Modifier.weight(1f).fillMaxWidth().background(DarkColors.terminal).onSizeChanged { area = it }) {
            AndroidView(
                factory = { ScreenSurface(it, feed).also { created -> view = created } },
                update = { it.place(placement, region, pictureSize) },
                modifier = Modifier.fillMaxSize()
            )
            if (placement != null) ScreenPointer(feed, placement, area)
            KodosiTheme(dark = true) {
                val fading = fade<Float>()
                val cover = when {
                    access?.screen == false -> ScreenCover.UNRECORDED
                    undecodable -> ScreenCover.UNDECODABLE
                    failure != null && !picture -> ScreenCover.FAILED
                    !picture -> ScreenCover.WAITING
                    else -> null
                }
                AnimatedContent(
                    cover,
                    Modifier.fillMaxSize(),
                    transitionSpec = { (fadeIn(fading) togetherWith fadeOut(fading)).using(null) },
                    label = "screen cover"
                ) { shown ->
                    when (shown) {
                        ScreenCover.UNRECORDED -> EmptyState(
                            stringResource(R.string.screen_unrecorded, computer),
                            stringResource(R.string.screen_unrecorded_detail, computer),
                            AppSymbol.DISPLAY
                        )

                        ScreenCover.UNDECODABLE -> EmptyState(
                            stringResource(R.string.screen_undecodable),
                            symbol = AppSymbol.DISPLAY
                        )

                        ScreenCover.FAILED -> EmptyState(
                            stringResource(R.string.screen_failed),
                            failure,
                            AppSymbol.DISPLAY
                        ) {
                            KodosiTextButton(onClick = feed::retry) { Text(stringResource(R.string.try_again)) }
                        }

                        ScreenCover.WAITING -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                            ProgressCaption(stringResource(R.string.screen_waiting))
                        }

                        null -> Unit
                    }
                }
                ReconnectingNotice(
                    cover == null && reconnecting,
                    Modifier.align(Alignment.TopCenter).padding(top = 10.dp),
                    reason = failure,
                    retry = feed::retry
                )
            }
        }
    }
    if (explaining) {
        KodosiDialog(
            onDismissRequest = { explaining = false },
            title = { Text(stringResource(R.string.screen_view_only)) },
            text = { Text(stringResource(R.string.screen_view_only_explained, computer)) },
            confirmButton = {
                KodosiTextButton(onClick = { explaining = false }) { Text(stringResource(R.string.close)) }
            }
        )
    }
}

@Composable
private fun ScreenSources(feed: ScreenFeed, modifier: Modifier) {
    val colors = KodosiTheme.colors
    val displays by feed.displays.collectAsStateWithLifecycle()
    val windows by feed.windows.collectAsStateWithLifecycle()
    val source by feed.source.collectAsStateWithLifecycle()
    var choosing by remember { mutableStateOf(false) }
    val mainDisplay = stringResource(R.string.screen_main_display)
    val numbered = displays.indices.map { index ->
        if (index == 0) mainDisplay else stringResource(R.string.screen_display, index + 1)
    }
    val shown = source
    val name = when (shown?.kind) {
        "display" -> numbered.getOrNull(displays.indexOfFirst { it.id == shown.id }) ?: mainDisplay

        "window" -> windows.firstOrNull { it.id == shown.id }?.let { it.title.ifBlank { it.app } }
            ?: stringResource(R.string.screen_window)

        else -> stringResource(R.string.screen)
    }
    val chooseLabel = stringResource(R.string.screen_choose)
    Box(modifier) {
        Row(
            Modifier.heightIn(min = 48.dp).widthIn(max = 280.dp).clip(CircleShape).well(colors, CircleShape)
                .clickable(role = Role.Button) { choosing = true }
                .semantics { contentDescription = chooseLabel }
                .padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            AppIcon(
                if (shown?.kind == "window") AppSymbol.WINDOW else AppSymbol.DISPLAY,
                Modifier.size(13.dp),
                tint = colors.inkMuted
            )
            Text(
                name,
                Modifier.weight(1f, fill = false),
                style = KodosiType.footnote,
                color = colors.ink,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            AppIcon(AppSymbol.CHEVRON_DOWN, Modifier.size(10.dp), tint = colors.inkFaint)
        }
        KodosiMenu(choosing, onDismissRequest = { choosing = false }) {
            displays.forEachIndexed { index, display ->
                KodosiMenuItem(text = { Text(numbered[index]) }, onClick = {
                    choosing = false
                    feed.show(ScreenFeed.Source("display", display.id))
                })
            }
            windows.forEach { window ->
                KodosiMenuItem(text = {
                    Text(
                        if (window.title.isBlank()) window.app else "${window.title} – ${window.app}",
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }, onClick = {
                    choosing = false
                    feed.show(ScreenFeed.Source("window", window.id))
                })
            }
        }
    }
}

// The computer's pointer with its shape, growing and shrinking with the picture but kept large
// enough to find.
@Composable
private fun ScreenPointer(feed: ScreenFeed, placement: Placement, area: IntSize) {
    val pointer by feed.pointer.collectAsStateWithLifecycle()
    val shapes by feed.shapes.collectAsStateWithLifecycle()
    val density = LocalDensity.current
    val at = pointer ?: return
    val shape = shapes[at.shape] ?: return
    val scale = maxOf(placement.fit(area.width.toFloat(), area.height.toFloat()), SMALLEST_POINTER * density.density)
    val spot = placement.viewRect(FrameRegion(at.x, at.y, 0f, 0f), area.width.toFloat(), area.height.toFloat())
    Image(
        shape.image,
        null,
        Modifier
            .offset {
                IntOffset(
                    (spot.x - shape.hotspotX * scale).roundToInt(),
                    (spot.y - shape.hotspotY * scale).roundToInt()
                )
            }
            .size(with(density) { (shape.width * scale).toDp() }, with(density) { (shape.height * scale).toDp() })
    )
}

// The smallest pointer, in device-independent pixels per point of the computer.
private const val SMALLEST_POINTER = 0.6f
