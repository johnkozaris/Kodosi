package com.kodosi.mobile.chat

import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import android.provider.Settings
import androidx.compose.animation.core.animateFloatAsState
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
import com.kodosi.mobile.ui.KodosiSendKey
import com.kodosi.mobile.ui.KodosiStopKey
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.snappy
import kotlinx.coroutines.delay

/** The quiet after which Send starts to show the pause, so that a breath does not flash its ring. */
private const val RING_DELAY_MILLIS = 800L

// Fills about 30 times a second, which looks smooth on a slow ring and costs less than every frame.
private const val RING_STEP_MILLIS = 33L

/**
 * The talk bar in place of the composer: the bars of the voice that talk mode hears and what it
 * does, then the stop key in the place of the talk key, and Send in its own place with its pause
 * ring. Send sends the words heard, or else words that the person typed before talking.
 */
@Composable
internal fun TalkBar(
    talk: ChatTalk,
    asking: Boolean,
    canWrite: Boolean,
    typed: Boolean,
    stop: () -> Unit,
    sendTyped: () -> Unit
) {
    val colors = KodosiTheme.colors
    val heard = !talk.words.isEmpty
    val sendable = (heard || typed) && canWrite
    val waiting = talk.sendAsked != null
    Row(
        Modifier.fillMaxWidth().padding(10.dp)
            .raised(colors, RoundedCornerShape(20.dp), fill = colors.lifted, elevation = Elevation.LIFTED)
            .padding(5.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Row(
            Modifier.weight(1f).padding(start = 12.dp)
                .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically
        ) {
            VoiceLevel(talk)
            Text(
                stringResource(
                    when {
                        asking && sendable -> R.string.talk_tap_send
                        talk.listening -> R.string.talk_listening
                        else -> R.string.talk_getting_ready
                    }
                ),
                style = KodosiType.body,
                color = colors.inkMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(start = 8.dp)
            )
        }
        KodosiStopKey(stop, stringResource(R.string.talk_stop), colors.accent)
        Box(contentAlignment = Alignment.Center) {
            KodosiSendKey(sendable, working = waiting, onClick = if (heard) talk::send else sendTyped)
            PauseRing(talk.takeIf { heard && canWrite && !waiting && !asking })
        }
    }
}

private val VoiceBars = floatArrayOf(0.55f, 0.85f, 1f, 0.75f, 0.5f)

/** Bars that rise with the voice that talk mode hears, flat and faint while it gets ready. */
@Composable
private fun VoiceLevel(talk: ChatTalk) {
    val colors = KodosiTheme.colors
    val live = talk.listening
    val level by animateFloatAsState(if (live) talk.level else 0f, snappy(), label = "voice level")
    Canvas(Modifier.size(width = 22.dp, height = 18.dp)) {
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

/**
 * The message so far as the person's own, not sent yet: the words that they typed before talking,
 * then the words heard. A tap opens it for typing.
 */
@Composable
internal fun TalkBubble(talk: ChatTalk, typed: String, type: () -> Unit) {
    val before = typed.trim()
    val shown = (before to talk.words).takeIf { talk.on && (before.isNotEmpty() || !talk.words.isEmpty) }
    val leaving = remember { mutableStateOf("" to HeardWords()) }
    SideEffect { if (shown != null) leaving.value = shown }
    Unfolding(shown != null) { (shown ?: leaving.value).let { (before, words) -> HeardBubble(before, words, type) } }
}

@Composable
private fun HeardBubble(typed: String, words: HeardWords, type: () -> Unit) {
    val colors = KodosiTheme.colors
    val label = stringResource(R.string.talk_unsent)
    val message = joinedWords(typed, words.message)
    val settled = message.length - (words.message.length - words.settled.length)
    val text = buildAnnotatedString {
        append(message)
        addStyle(SpanStyle(color = colors.ink), 0, settled)
        addStyle(SpanStyle(color = colors.inkFaint), settled, message.length)
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
