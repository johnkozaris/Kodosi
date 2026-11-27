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
import androidx.compose.ui.text.font.FontWeight
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
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ReconnectingNotice
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.well
import kotlin.math.roundToInt

// The screen of the terminal's computer, which its owner watches and controls from this device. It
// says only what someone has to act on or wait for, and rests on the last picture while the
// computer's own pictures come back.
@Composable
fun ScreenView(model: AppModel, session: String, computer: String, shared: Boolean, offline: Boolean) {
    val feed = remember(session) { model.video.screen(session) }
    val picture by feed.picture.collectAsStateWithLifecycle()
    val live by feed.live.collectAsStateWithLifecycle()
    val pictureSize by feed.pictureSize.collectAsStateWithLifecycle()
    val region by feed.region.collectAsStateWithLifecycle()
    val failure by feed.failure.collectAsStateWithLifecycle()
    val undecodable by feed.undecodable.collectAsStateWithLifecycle()
    val access by feed.access.collectAsStateWithLifecycle()
    val away by feed.away.collectAsStateWithLifecycle()
    val control by feed.control.collectAsStateWithLifecycle()
    val zoom by feed.zoom.collectAsStateWithLifecycle()
    val placing by feed.placing.collectAsStateWithLifecycle()
    var view by remember { mutableStateOf<ScreenSurface?>(null) }
    var area by remember { mutableStateOf(IntSize.Zero) }
    // Before the feed shows, so that it opens no pipe to a screen that its computer does not share.
    DisposableEffect(feed, shared) {
        feed.share(shared)
        onDispose {}
    }
    LifecycleResumeEffect(feed) {
        feed.show(true)
        onPauseOrDispose { feed.show(false) }
    }
    CopiesToClipboard(feed)
    val shows = screenShows(
        shared,
        offline,
        away,
        recording = access?.screen != false,
        failed = failure != null || undecodable,
        live = live && picture,
        kept = picture
    )
    val controls = access?.input != false
    val placement = remember(zoom, placing, area, region, control) {
        feed.placement(area.width.toFloat(), area.height.toFloat())
    }
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            ScreenSources(feed, Modifier.weight(1f, fill = false))
            if (zoom > 1f) ActionIcon(AppSymbol.SHRINK, R.string.screen_whole, feed::showWhole)
            Box(Modifier.weight(1f))
            KeyboardButton(view, enabled = picture && controls)
        }
        if (!controls && (shows == ScreenShows.LIVE || shows == ScreenShows.RESTING)) {
            Text(
                stringResource(R.string.screen_uncontrolled, computer),
                Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, bottom = 6.dp),
                style = KodosiType.footnote,
                fontWeight = FontWeight.Medium,
                color = KodosiTheme.colors.caution
            )
        }
        Box(Modifier.weight(1f).fillMaxWidth().background(DarkColors.terminal).onSizeChanged { area = it }) {
            AndroidView(
                factory = { ScreenSurface(it, feed).also { created -> view = created } },
                update = {
                    it.place(placement, region, pictureSize)
                    it.rest(shows == ScreenShows.RESTING)
                },
                modifier = Modifier.fillMaxSize()
            )
            if (placement != null && (shows == ScreenShows.LIVE || shows == ScreenShows.RESTING)) {
                ScreenPointer(feed, placement, area)
            }
            KodosiTheme(dark = true) {
                val fading = fade<Float>()
                AnimatedContent(
                    shows,
                    Modifier.fillMaxSize(),
                    transitionSpec = { (fadeIn(fading) togetherWith fadeOut(fading)).using(null) },
                    contentKey = {
                        it.takeUnless { shown -> shown == ScreenShows.LIVE || shown == ScreenShows.RESTING }
                    },
                    label = "screen state"
                ) { shown ->
                    ScreenState(shown, computer, feed::retry)
                }
                ReconnectingNotice(
                    shows == ScreenShows.RESTING,
                    Modifier.align(Alignment.TopCenter).padding(top = 10.dp),
                    patience = RECONNECTING_PATIENCE
                )
            }
        }
    }
}

@Composable
private fun ScreenState(shows: ScreenShows, computer: String, retry: () -> Unit) {
    when (shows) {
        ScreenShows.SHARING_OFF -> VideoNote(AppSymbol.UNSHARED, stringResource(R.string.screen_sharing_off, computer))

        ScreenShows.OFFLINE -> VideoNote(AppSymbol.OFFLINE, stringResource(R.string.computer_offline, computer))

        ScreenShows.LOCKED -> VideoNote(AppSymbol.LOCK, stringResource(R.string.screen_locked, computer))

        ScreenShows.ANOTHER_ACCOUNT -> VideoNote(
            AppSymbol.FRIENDS,
            stringResource(R.string.screen_another_account, computer)
        )

        ScreenShows.ASLEEP -> VideoNote(AppSymbol.MOON, stringResource(R.string.screen_asleep, computer))

        ScreenShows.UNRECORDED -> VideoNote(
            AppSymbol.RECORDING,
            stringResource(R.string.screen_allow_recording, computer)
        )

        ScreenShows.CANT -> VideoNote(AppSymbol.WARNING, stringResource(R.string.screen_cant_show), retry)

        ScreenShows.OPENING -> FirstWait()

        ScreenShows.LIVE, ScreenShows.RESTING -> Unit
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
