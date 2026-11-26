package com.kodosi.mobile.canvas

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiSpinner
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.well
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.delay

// What the screen view shows, from the first that applies. Away states end by themselves, so they
// offer nothing to do; only a failure that nothing mends by itself offers to try again.
internal enum class ScreenShows {
    OFFLINE,
    SHARING_OFF,
    LOCKED,
    ANOTHER_ACCOUNT,
    ASLEEP,
    UNRECORDED,
    CANT,
    LIVE,
    RESTING,
    OPENING
}

internal fun screenShows(
    shared: Boolean,
    offline: Boolean,
    away: ScreenFeed.Away?,
    recording: Boolean,
    failed: Boolean,
    live: Boolean,
    kept: Boolean
): ScreenShows = when {
    offline -> ScreenShows.OFFLINE
    !shared -> ScreenShows.SHARING_OFF
    away == ScreenFeed.Away.LOCKED -> ScreenShows.LOCKED
    away == ScreenFeed.Away.ANOTHER_ACCOUNT -> ScreenShows.ANOTHER_ACCOUNT
    away == ScreenFeed.Away.ASLEEP -> ScreenShows.ASLEEP
    !recording -> ScreenShows.UNRECORDED
    failed -> ScreenShows.CANT
    live -> ScreenShows.LIVE
    kept -> ScreenShows.RESTING
    else -> ScreenShows.OPENING
}

// What the browser view shows, from the first that applies.
internal enum class BrowserShows { OFFLINE, CANT, GETTING, LIVE, RESTING, OPENING }

internal fun browserShows(offline: Boolean, failed: Boolean, getting: Boolean, live: Boolean, kept: Boolean) = when {
    offline -> BrowserShows.OFFLINE
    failed -> BrowserShows.CANT
    getting -> BrowserShows.GETTING
    live -> BrowserShows.LIVE
    kept -> BrowserShows.RESTING
    else -> BrowserShows.OPENING
}

// How long a picture rests before the word says that it reconnects.
internal val RECONNECTING_PATIENCE = 6.seconds

// One line on the canvas, over no picture, when someone has to do something or wait for something.
@Composable
internal fun VideoNote(symbol: AppSymbol, line: String, retry: (() -> Unit)? = null) {
    val colors = KodosiTheme.colors
    Column(
        Modifier.fillMaxSize().background(colors.terminal).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Box(Modifier.size(44.dp).well(colors, RoundedCornerShape(13.dp)), contentAlignment = Alignment.Center) {
            AppIcon(symbol, Modifier.size(20.dp))
        }
        Text(line, style = KodosiType.headline, color = colors.ink, textAlign = TextAlign.Center)
        if (retry != null) {
            KodosiButton(
                stringResource(R.string.try_again),
                retry,
                kind = ButtonKind.SECONDARY,
                size = ButtonSize.SMALL
            )
        }
    }
}

// The first picture takes a moment, so the spinner comes only if it takes longer.
@Composable
internal fun FirstWait() {
    var long by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(FIRST_WAIT_MS)
        long = true
    }
    if (long) Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { KodosiSpinner(Modifier.size(18.dp)) }
}

// Text copied on the other side lands on this device's clipboard, which Android confirms by itself.
@Composable
internal fun CopiesToClipboard(feed: VideoFeed) {
    val context = LocalContext.current
    LaunchedEffect(feed) {
        feed.copies.collect {
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("", it))
        }
    }
}

private const val FIRST_WAIT_MS = 500L
