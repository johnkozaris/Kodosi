package com.kodosi.mobile.sharing

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathMeasure
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.snappy
import com.kodosi.mobile.ui.well
import kotlin.math.ceil
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Asks the owner before a change lets someone new into their terminals or projects: a warning that
 * draws the eye, which the owner answers twice, with the tick of its consent and then the confirm.
 */
@Composable
fun SharingQuestion(approval: SharingApproval, answer: (allow: Boolean) -> Unit) {
    val readMillis = approval.readSeconds * 1000L
    // Saved under the approval's id, so a question never takes over the tick or the read time of
    // the one before it when the activity comes back.
    val gate = rememberSaveable(
        approval.approvalId,
        saver = ConfirmGate.saver(readMillis),
        key = approval.approvalId
    ) { ConfirmGate(readMillis) }
    // A tap or a key meant for what was there a moment ago does not answer a new question.
    var settled by remember(approval.approvalId) { mutableStateOf(false) }
    LaunchedEffect(approval.approvalId) {
        delay(SETTLE_MILLIS)
        settled = true
    }
    val refuse = { if (settled) answer(false) }
    Dialog(
        onDismissRequest = refuse,
        properties = DialogProperties(dismissOnClickOutside = false, usePlatformDefaultWidth = false)
    ) {
        key(approval.approvalId) { QuestionPanel(approval, gate, answer, refuse) }
    }
}

/**
 * When the confirm of a question works: once the owner ticked its consent and the question was in
 * front of them for its whole read time in one stretch. Leaving the front before the time is up
 * starts it over; once it is up, it stays up. Times are frame milliseconds.
 */
@Stable
internal class ConfirmGate(private val readMillis: Long, ticked: Boolean = false, read: Boolean = false) {
    var ticked by mutableStateOf(ticked)

    var read by mutableStateOf(read)
        private set

    /** The share of the read time still to go, from 1 down to 0. */
    var left by mutableFloatStateOf(if (read) 0f else 1f)
        private set

    private var frontSince: Long? = null

    val opens: Boolean get() = ticked && read

    fun front(now: Long) {
        if (read) return
        val since = frontSince ?: now.also { frontSince = it }
        val shown = now - since
        read = shown >= readMillis
        left = if (read) 0f else 1f - shown.toFloat() / readMillis
    }

    fun away() {
        frontSince = null
        if (!read) left = 1f
    }

    companion object {
        fun saver(readMillis: Long) = listSaver<ConfirmGate, Boolean>(
            save = { listOf(it.ticked, it.read) },
            restore = { (ticked, read) -> ConfirmGate(readMillis, ticked, read) }
        )
    }
}

@Composable
private fun QuestionPanel(
    approval: SharingApproval,
    gate: ConfirmGate,
    answer: (allow: Boolean) -> Unit,
    refuse: () -> Unit
) {
    val colors = KodosiTheme.colors
    val reduced = reducedMotion()
    val popping = snappy<Float>()
    val haptics = LocalHapticFeedback.current
    val rise = remember { Animatable(if (reduced) 1f else 0f) }
    val pop = remember { Animatable(if (reduced) 1f else 0.6f) }
    val wiggle = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        haptics.performHapticFeedback(HapticFeedbackType.Reject)
        if (reduced) return@LaunchedEffect
        launch { rise.animateTo(1f, Rising) }
        delay(MARK_DELAY_MILLIS)
        launch { pop.animateTo(1f, popping) }
        wiggle.animateTo(0f, Wiggle)
    }
    val resumed = LocalLifecycleOwner.current.lifecycle.currentStateAsState().value
        .isAtLeast(Lifecycle.State.RESUMED)
    val front = resumed && LocalWindowInfo.current.isWindowFocused
    LaunchedEffect(gate, front) {
        if (front) {
            try {
                while (!gate.read) withFrameMillis(gate::front)
            } finally {
                gate.away()
            }
        }
    }
    val spoken = listOfNotNull(approval.title, approval.asker, approval.message).joinToString("\n")
    Box(
        Modifier.fillMaxWidth().semantics { testTagsAsResourceId = true }
            .padding(horizontal = 16.dp, vertical = 32.dp),
        contentAlignment = Alignment.Center
    ) {
        Column(
            Modifier.widthIn(max = 440.dp).fillMaxWidth()
                .graphicsLayer {
                    val shown = rise.value
                    alpha = shown.coerceIn(0f, 1f)
                    scaleX = 0.95f + 0.05f * shown
                    scaleY = scaleX
                    translationY = (1f - shown) * RiseDistance.toPx()
                }
                .raised(colors, RoundedCornerShape(Radius.sheet), elevation = Elevation.FLOATING)
                .semantics { paneTitle = spoken }
                .testTag("sharing.question")
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp)
        ) {
            Column(
                Modifier.weight(1f, fill = false).fillMaxWidth().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(14.dp)
            ) {
                CautionMark({ pop.value }, { wiggle.value })
                Text(approval.title, Modifier.semantics { heading() }, style = KodosiType.title, color = colors.ink)
                approval.asker?.let { Asker(it) }
                Text(approval.message, style = KodosiType.body, color = colors.ink)
                Consent(approval.consent, gate.ticked) { gate.ticked = it }
            }
            FlowRow(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                KodosiButton(
                    stringResource(R.string.cancel),
                    onClick = refuse,
                    modifier = Modifier.testTag("sharing.question.cancel"),
                    kind = ButtonKind.SECONDARY
                )
                KodosiButton(
                    onClick = { answer(true) },
                    modifier = Modifier.testTag("sharing.question.confirm"),
                    kind = ButtonKind.DANGER,
                    enabled = gate.opens
                ) {
                    Text(
                        approval.confirm,
                        Modifier.weight(1f, fill = false),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                    ReadRing(!gate.read, { gate.left }, approval.readSeconds)
                }
            }
        }
    }
}

@Composable
private fun CautionMark(pop: () -> Float, wiggle: () -> Float) {
    val colors = KodosiTheme.colors
    Box(
        Modifier.size(MarkSize)
            .graphicsLayer {
                scaleX = pop()
                scaleY = scaleX
            }
            .background(colors.caution, RoundedCornerShape(MarkSize * 0.3f)),
        contentAlignment = Alignment.Center
    ) {
        Canvas(Modifier.size(24.dp).graphicsLayer { rotationZ = wiggle() }) {
            scale(size.width / 24f, pivot = Offset.Zero) {
                drawPath(Shield, colors.onDanger)
                drawLine(
                    colors.caution,
                    Offset(12f, 7.5f),
                    Offset(12f, 12.5f),
                    strokeWidth = 2.2f,
                    cap = StrokeCap.Round
                )
                drawCircle(colors.caution, radius = 1.2f, center = Offset(12f, 16f))
            }
        }
    }
}

@Composable
private fun Asker(asker: String) {
    val colors = KodosiTheme.colors
    val glyph = with(LocalDensity.current) { 13.sp.toDp() }
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        AppIcon(
            AppSymbol.TERMINAL,
            // Puts the middle of the glyph level with the middle of the first line's capitals.
            Modifier.size(glyph).alignBy { (it.measuredHeight * 0.85f).roundToInt() },
            tint = colors.caution
        )
        Text(
            asker,
            Modifier.alignByBaseline(),
            style = KodosiType.footnote.copy(fontWeight = FontWeight.SemiBold),
            color = colors.caution
        )
    }
}

@Composable
private fun Consent(consent: String, ticked: Boolean, tick: (Boolean) -> Unit) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(Radius.md)
    Row(
        Modifier.fillMaxWidth().testTag("sharing.question.consent")
            .well(colors, shape)
            .border(1.5.dp, if (focused) colors.accent.copy(alpha = 0.55f) else Color.Transparent, shape)
            .toggleable(ticked, interaction, indication = null, role = Role.Checkbox, onValueChange = tick)
            .defaultMinSize(minHeight = 48.dp)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        TickBox(ticked)
        Text(consent, style = KodosiType.body, color = colors.ink)
    }
}

@Composable
private fun TickBox(ticked: Boolean) {
    val colors = KodosiTheme.colors
    val drawing = remember { Animatable(if (ticked) 1f else 0f) }
    val fading = fade<Float>()
    LaunchedEffect(ticked) { if (ticked) drawing.animateTo(1f, fading) else drawing.snapTo(0f) }
    val shape = RoundedCornerShape(5.dp)
    val face = if (ticked) colors.accent else lerp(colors.well, colors.shadow, if (colors.isDark) 0.5f else 0.06f)
    Spacer(
        Modifier.size(20.dp)
            .background(face, shape)
            .border(1.5.dp, if (ticked) colors.accent else colors.inkFaint, shape)
            .drawWithCache {
                val unit = size.width / 24f
                val check = Path().apply {
                    moveTo(4.6f * unit, 12.8f * unit)
                    lineTo(9.6f * unit, 17.6f * unit)
                    lineTo(19.4f * unit, 6.8f * unit)
                }
                val measure = PathMeasure().apply { setPath(check, forceClosed = false) }
                val drawn = Path()
                val stroke = Stroke(2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round)
                onDrawBehind {
                    drawn.reset()
                    if (measure.getSegment(0f, measure.length * drawing.value, drawn)) {
                        drawPath(drawn, colors.onAccent, style = stroke)
                    }
                }
            }
    )
}

/** The read time still to go, as a ring that empties clockwise around the whole seconds left. */
@Composable
private fun ReadRing(counting: Boolean, left: () -> Float, seconds: Int) {
    val colors = KodosiTheme.colors
    val fading = fade<Float>()
    val sizing = fade<IntSize>()
    AnimatedVisibility(
        counting,
        Modifier.clearAndSetSemantics {},
        exit = fadeOut(fading) + scaleOut(fading) + shrinkHorizontally(sizing),
        label = "read ring"
    ) {
        val ring = with(LocalDensity.current) { 18.sp.toDp() }
        val shown by remember(seconds) { derivedStateOf { ceil(left() * seconds).toInt() } }
        Row {
            Spacer(Modifier.width(8.dp))
            Box(
                Modifier.size(ring).drawBehind {
                    val stroke = 2.dp.toPx()
                    val circle = Size(size.width - stroke, size.height - stroke)
                    drawCircle(colors.hairline, radius = circle.width / 2, style = Stroke(stroke))
                    val share = left()
                    if (share > 0f) {
                        drawArc(
                            colors.inkMuted,
                            startAngle = 270f + 360f * (1f - share),
                            sweepAngle = 360f * share,
                            useCenter = false,
                            topLeft = Offset(stroke / 2, stroke / 2),
                            size = circle,
                            style = Stroke(stroke, cap = StrokeCap.Round)
                        )
                    }
                },
                contentAlignment = Alignment.Center
            ) {
                Text(
                    "$shown",
                    style = KodosiType.caption.copy(fontWeight = FontWeight.SemiBold, fontFeatureSettings = "tnum"),
                    color = colors.inkMuted,
                    maxLines = 1,
                    softWrap = false
                )
            }
        }
    }
}

private val Shield = PathParser()
    .parsePathString("M12 2.5 L19.5 5.5 V11 Q19.5 17.5 12 21.5 Q4.5 17.5 4.5 11 V5.5 Z")
    .toPath()

private val MarkSize = 44.dp
private val RiseDistance = 18.dp
private val Rising = spring<Float>(dampingRatio = 0.875f, stiffness = 420f)
private val Wiggle = keyframes<Float> {
    durationMillis = 500
    0f at 0 using FastOutSlowInEasing
    8f at 62 using FastOutSlowInEasing
    -8f at 187 using FastOutSlowInEasing
    8f at 312 using FastOutSlowInEasing
    -8f at 437 using FastOutSlowInEasing
}
private const val MARK_DELAY_MILLIS = 100L
private const val SETTLE_MILLIS = 500L
