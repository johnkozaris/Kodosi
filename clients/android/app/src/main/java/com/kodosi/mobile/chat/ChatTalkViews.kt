package com.kodosi.mobile.chat

import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import android.provider.Settings
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.KodosiQuietKey
import com.kodosi.mobile.ui.KodosiSendKey
import com.kodosi.mobile.ui.KodosiStopKey
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.snappy
import kotlinx.coroutines.delay

/** The quiet after which Send starts to show the pause, so that a breath does not flash its ring. */
private const val RING_DELAY_MILLIS = 800L

// Fills about 30 times a second, which looks smooth on a slow ring and costs less than every frame.
private const val RING_STEP_MILLIS = 33L

/**
 * The talk bar in place of the composer: the key back to typing, the bars of the voice that talk
 * mode hears, then Send with its pause ring, or Stop while the agent works and nothing is heard.
 */
@Composable
internal fun TalkBar(
    talk: ChatTalk,
    working: Boolean,
    asking: Boolean,
    canWrite: Boolean,
    type: () -> Unit,
    stop: () -> Unit
) {
    val colors = KodosiTheme.colors
    val heard = !talk.words.isEmpty
    val fading = fade<Float>()
    val popping = snappy<Float>()
    Row(
        Modifier.fillMaxWidth().padding(10.dp)
            .raised(colors, RoundedCornerShape(20.dp), fill = colors.lifted, elevation = Elevation.LIFTED)
            .padding(5.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        KodosiQuietKey(AppSymbol.KEYBOARD, stringResource(R.string.talk_type_instead), type)
        Row(
            Modifier.weight(1f),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically
        ) {
            VoiceLevel(talk)
            AnimatedVisibility(
                asking && heard,
                enter = fadeIn(fading) + expandHorizontally(fade()),
                exit = fadeOut(fading) + shrinkHorizontally(fade()),
                label = "answer hint"
            ) {
                Text(
                    stringResource(R.string.talk_tap_send),
                    style = KodosiType.footnote,
                    color = colors.inkMuted,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(start = 8.dp).semantics { liveRegion = LiveRegionMode.Polite }
                )
            }
        }
        AnimatedContent(
            working && !heard,
            transitionSpec = {
                (fadeIn(fading) + scaleIn(popping, initialScale = 0.9f)) togetherWith
                    (fadeOut(fading) + scaleOut(popping, targetScale = 0.9f))
            },
            label = "talk key"
        ) { stops ->
            if (stops) {
                KodosiStopKey(stop)
            } else {
                val sendable = heard && canWrite
                val waiting = talk.sendAsked != null
                Box(contentAlignment = Alignment.Center) {
                    KodosiSendKey(sendable, working = waiting, onClick = talk::send)
                    PauseRing(talk.takeIf { sendable && !waiting && !asking })
                }
            }
        }
    }
}

private val VoiceBars = floatArrayOf(0.55f, 0.85f, 1f, 0.75f, 0.5f)

/** Bars that rise with the voice that talk mode hears, flat and faint while it gets ready. */
@Composable
private fun VoiceLevel(talk: ChatTalk) {
    val colors = KodosiTheme.colors
    val live = talk.listening
    val state = stringResource(if (live) R.string.talk_listening else R.string.talk_getting_ready)
    val level by animateFloatAsState(if (live) talk.level else 0f, snappy(), label = "voice level")
    Canvas(
        Modifier.size(width = 22.dp, height = 18.dp).semantics {
            contentDescription = state
            liveRegion = LiveRegionMode.Polite
        }
    ) {
        val bar = 2.5.dp.toPx()
        val gap = (size.width - VoiceBars.size * bar) / (VoiceBars.size - 1)
        VoiceBars.forEachIndexed { index, reach ->
            val height = 3.dp.toPx() + 13.dp.toPx() * level * reach
            drawRoundRect(
                if (live) colors.ink else colors.inkFaint,
                topLeft = Offset(index * (bar + gap), (size.height - height) / 2),
                size = Size(bar, height),
                cornerRadius = CornerRadius(bar / 2)
            )
        }
    }
}

/**
 * The ring around Send that fills during the pause that sends the words, from the last time that
 * [talk] heard the person, or none without a pause that can send.
 */
@Composable
private fun PauseRing(talk: ChatTalk?) {
    val colors = KodosiTheme.colors
    val reduced = reducedMotion()
    val since = talk?.quietSince
    var filled by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(since, reduced) {
        filled = 0f
        if (since == null) return@LaunchedEffect
        while (filled < 1f) {
            val quiet = SystemClock.uptimeMillis() - since - RING_DELAY_MILLIS
            if (quiet < 0) {
                delay(-quiet)
                continue
            }
            filled = if (reduced) {
                1f
            } else {
                (quiet.toFloat() / (TalkTurn.PAUSE_MILLIS - RING_DELAY_MILLIS)).coerceAtMost(1f)
            }
            delay(RING_STEP_MILLIS)
        }
    }
    Canvas(Modifier.size(38.dp)) {
        if (filled <= 0f) return@Canvas
        val line = Stroke(2.dp.toPx(), cap = StrokeCap.Round)
        val inset = line.width / 2
        drawArc(
            colors.accent,
            -90f,
            360f * filled,
            false,
            Offset(inset, inset),
            Size(size.width - line.width, size.height - line.width),
            style = line
        )
    }
}

/** The words heard so far as the person's own message, not sent yet; a tap opens them for typing. */
@Composable
internal fun TalkBubble(talk: ChatTalk, type: () -> Unit) {
    val heard = talk.words.takeIf { talk.on && !it.isEmpty }
    val leaving = remember { mutableStateOf(HeardWords()) }
    SideEffect { if (heard != null) leaving.value = heard }
    Unfolding(heard != null) { HeardBubble(heard ?: leaving.value, type) }
}

@Composable
private fun HeardBubble(words: HeardWords, type: () -> Unit) {
    val colors = KodosiTheme.colors
    val label = stringResource(R.string.talk_unsent)
    val message = words.message
    val text = buildAnnotatedString {
        append(message)
        addStyle(SpanStyle(color = colors.ink), 0, words.settled.length)
        addStyle(SpanStyle(color = colors.inkFaint), words.settled.length, message.length)
    }
    Row(
        Modifier.fillMaxWidth().padding(start = 48.dp, end = 10.dp, top = 2.dp, bottom = 2.dp),
        horizontalArrangement = Arrangement.End
    ) {
        Text(
            text,
            style = KodosiType.body,
            color = colors.ink,
            modifier = Modifier.clip(MineShape)
                .clickable(
                    onClickLabel = stringResource(R.string.talk_type_instead),
                    role = Role.Button,
                    onClick = type
                )
                .semantics {
                    contentDescription = label
                    stateDescription = message
                }
                .background(colors.accentSoft)
                .heightIn(max = 168.dp)
                .verticalScroll(rememberScrollState(), reverseScrolling = true)
                .padding(horizontal = 12.dp, vertical = 8.dp)
        )
    }
}

/** Why talk mode can't listen, with the way to Settings when it needs the microphone. */
@Composable
internal fun TalkNoteLine(talk: ChatTalk) {
    val note = talk.note
    val leaving = remember { mutableStateOf(note) }
    SideEffect { if (note != null) leaving.value = note }
    Unfolding(note != null) { (note ?: leaving.value)?.let { NoteLine(it) { talk.note = null } } }
}

@Composable
private fun NoteLine(note: TalkNote, dismiss: () -> Unit) {
    val colors = KodosiTheme.colors
    val context = LocalContext.current
    val words = when (note) {
        TalkNote.MICROPHONE -> R.string.talk_allow_microphone
        TalkNote.UNAVAILABLE -> R.string.talk_unavailable
        TalkNote.FAILED -> R.string.talk_failed
    }
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 4.dp)
            .semantics { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            stringResource(words),
            style = KodosiType.footnote,
            color = colors.inkMuted,
            modifier = Modifier.weight(1f)
        )
        if (note == TalkNote.MICROPHONE) {
            KodosiTextButton(onClick = {
                context.startActivity(
                    Intent(
                        Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                        Uri.fromParts("package", context.packageName, null)
                    )
                )
                dismiss()
            }) { Text(stringResource(R.string.open_settings)) }
        }
        ActionIcon(AppSymbol.CLOSE, R.string.close, dismiss, face = 24.dp)
    }
}
