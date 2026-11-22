package com.kodosi.mobile.ui

import android.os.SystemClock
import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.repeatable
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.withInfiniteAnimationFrameMillis
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.progressSemantics
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.FloatState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.MotionDurationScale
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.toRect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.StatusForm
import kotlinx.coroutines.delay

@Composable
fun reducedMotion(): Boolean = LocalReducedMotion.current

@Composable
private fun powerSaving(): Boolean = LocalPowerSaving.current

/** Art can stay on screen for a long time, so a moving sign in it stands still, as with reduced motion. */
@Composable
fun Still(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalReducedMotion provides true, content = content)
}

/** All moving signs step on one clock, so that a step of all of them is one frame and nothing draws between steps. */
@Composable
private fun signPhase(): FloatState? {
    if (reducedMotion() || powerSaving()) return null
    val phase = remember { mutableFloatStateOf(signPhaseAt(SystemClock.uptimeMillis(), SIGN_STEP_MILLIS)) }
    LaunchedEffect(phase) {
        val scale = coroutineContext[MotionDurationScale]?.scaleFactor ?: 1f
        val step = (SIGN_STEP_MILLIS * scale).toLong().coerceAtLeast(1)
        while (true) {
            delay(step - SystemClock.uptimeMillis() % step)
            withInfiniteAnimationFrameMillis { phase.floatValue = signPhaseAt(SystemClock.uptimeMillis(), step) }
        }
    }
    return phase
}

private fun signPhaseAt(now: Long, step: Long): Float = (now / step % SIGN_STEPS) / SIGN_STEPS.toFloat()

private fun breathAlpha(phase: Float): Float =
    0.45f + 0.55f * FastOutSlowInEasing.transform(if (phase < 0.5f) phase * 2 else (1 - phase) * 2)

private const val SIGN_STEPS = 32
private const val SIGN_STEP_MILLIS = 2400L / SIGN_STEPS
private const val BREATHS = 3

private val FadeEasing = CubicBezierEasing(0.23f, 1f, 0.32f, 1f)

/** A state that changes in place fades as in the other apps, and at once with reduced motion. */
@Composable
fun <T> fade(): FiniteAnimationSpec<T> = if (reducedMotion()) snap() else tween(200, easing = FadeEasing)

@Composable
fun <T> snappy(): FiniteAnimationSpec<T> =
    if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f)

/** A note that comes and goes opens and closes its own room, so that what is around it moves instead of jumping. */
@Composable
fun Unfolding(visible: Boolean, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val fading = fade<Float>()
    val sizing = fade<IntSize>()
    AnimatedVisibility(
        visible,
        modifier,
        enter = fadeIn(fading) + expandVertically(sizing),
        exit = fadeOut(fading) + shrinkVertically(sizing),
        label = "unfolding"
    ) { content() }
}

@Composable
fun Unfolding(message: String?, modifier: Modifier = Modifier, content: @Composable (String) -> Unit) {
    val last = remember { mutableStateOf(message) }
    SideEffect { if (message != null) last.value = message }
    Unfolding(message != null, modifier) { (message ?: last.value)?.let { content(it) } }
}

@Composable
fun KodosiSpinner(modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val turn = if (reducedMotion()) {
        null
    } else {
        rememberInfiniteTransition(label = "spinner").animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Restart),
            label = "spinner turn"
        )
    }
    Canvas(modifier.size(14.dp).progressSemantics()) {
        val stroke = Stroke(1.5.dp.toPx(), cap = StrokeCap.Round)
        val inset = stroke.width / 2
        val diameter = Size(size.width - stroke.width, size.height - stroke.width)
        drawArc(colors.hairline, 0f, 360f, false, Offset(inset, inset), diameter, style = stroke)
        drawArc(colors.accent, turn?.value ?: 0f, 100.8f, false, Offset(inset, inset), diameter, style = stroke)
    }
}

/** It rests after a few breaths, because its notice can stay for a whole outage. */
@Composable
fun BreathingDot(color: Color, modifier: Modifier = Modifier) {
    val still = reducedMotion() || powerSaving()
    val breath = remember { Animatable(1f) }
    LaunchedEffect(still) {
        breath.snapTo(if (still) 1f else 0f)
        if (!still) breath.animateTo(1f, repeatable(BREATHS, tween(1600, easing = EaseOut)))
    }
    Canvas(modifier.size(7.dp)) {
        val radius = size.minDimension / 2
        drawCircle(color, radius)
        val phase = breath.value
        if (phase < 1f) {
            val scale = 1f + 1.4f * phase
            drawCircle(
                color.copy(alpha = 0.55f * (1f - phase)),
                radius * scale,
                style = Stroke(1.5.dp.toPx() * scale)
            )
        }
    }
}

@Composable
fun ProgressCaption(text: String, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
        modifier = modifier
    ) {
        KodosiSpinner(Modifier.size(16.dp))
        Spacer(Modifier.width(10.dp))
        Text(text, style = KodosiType.callout, color = colors.inkMuted)
    }
}

@Composable
fun ReconnectingNotice(shown: Boolean, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val presence by animateFloatAsState(if (shown) 1f else 0f, snappy(), label = "reconnecting")
    if (presence <= 0f) return
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = modifier
            .graphicsLayer {
                alpha = presence.coerceIn(0f, 1f)
                translationY = (presence - 1f) * 8.dp.toPx()
            }
            .raised(colors, RoundedCornerShape(percent = 50), fill = colors.lifted, elevation = Elevation.FLOATING)
            .padding(start = 12.dp, top = 8.dp, end = 14.dp, bottom = 8.dp)
            .semantics { liveRegion = LiveRegionMode.Polite }
    ) {
        BreathingDot(colors.glowOrange)
        Text(stringResource(R.string.reconnecting), style = KodosiType.footnote, color = colors.ink)
    }
}

@Composable
fun ErrorNote(message: String, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier
            .clip(RoundedCornerShape(Radius.md))
            .background(colors.danger.copy(alpha = 0.14f))
            .padding(horizontal = 14.dp, vertical = 10.dp)
    ) {
        StatusSign(StatusForm.FAILED, size = 14.dp)
        Spacer(Modifier.width(10.dp))
        Text(message, style = KodosiType.footnote, color = colors.ink)
    }
}

@Composable
fun Wordmark(height: Dp, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val description = stringResource(R.string.app_name)
    Row(modifier = modifier.semantics { contentDescription = description }) {
        Image(
            painterResource(R.drawable.kodosi_wordmark),
            contentDescription = null,
            modifier = Modifier.height(height * 1.2f).width(height * 1.2f * WORDMARK_ASPECT)
        )
        Spacer(Modifier.width(height * 0.1f))
        Box(
            Modifier
                .padding(top = height * 0.32f)
                .size(width = height * 0.43f, height = height * 0.86f)
                .background(colors.accent, RoundedCornerShape(height * 0.06f))
        )
    }
}

private const val WORDMARK_ASPECT = 729f / 160f

enum class MarkActivity { ASLEEP, AWAKE, WORKING }

internal fun AgentKind.tint(account: Int? = null): Color {
    val shades = when (this) {
        AgentKind.CLAUDE -> listOf(0xFFD97757, 0xFFC4301C, 0xFFD38A0F, 0xFF6B3420, 0xFFC9456B)
        AgentKind.CODEX -> listOf(0xFF5F6B7A, 0xFF2E5A8C, 0xFF2F7D74, 0xFF3A3F4A, 0xFF6E5A7E)
        AgentKind.COPILOT -> listOf(0xFF8660D9, 0xFF4B3BB0, 0xFFB44BC8, 0xFF5A2F6E, 0xFF5B7BE0)
        AgentKind.CURSOR -> listOf(0xFF4E7C8C)
        AgentKind.OPENCODE -> listOf(0xFF6F6258)
        AgentKind.ANTIGRAVITY -> listOf(0xFF3D7F5A)
        AgentKind.SHELL -> listOf(0xFF54433A)
    }
    return Color(shades[(account?.takeIf { it > 0 } ?: 0) % shades.size])
}

@DrawableRes
private fun AgentKind.glyph(): Int? = when (this) {
    AgentKind.CLAUDE -> R.drawable.provider_claude
    AgentKind.CODEX -> R.drawable.provider_codex
    AgentKind.COPILOT -> R.drawable.provider_copilot
    AgentKind.CURSOR -> R.drawable.provider_cursor
    AgentKind.OPENCODE -> R.drawable.provider_opencode
    AgentKind.ANTIGRAVITY -> R.drawable.provider_antigravity
    AgentKind.SHELL -> null
}

@Composable
fun AgentMark(
    kind: AgentKind,
    size: Dp,
    modifier: Modifier = Modifier,
    account: Int? = null,
    activity: MarkActivity = MarkActivity.AWAKE
) {
    val colors = KodosiTheme.colors
    val tint = kind.tint(account)
    val asleep = activity == MarkActivity.ASLEEP
    val shape = RoundedCornerShape(size * 0.3f)
    val fill = if (asleep) {
        Brush.linearGradient(listOf(lerp(colors.raised, tint, 0.14f), lerp(colors.raised, tint, 0.3f)))
    } else {
        Brush.linearGradient(listOf(lerp(tint, Color.White, 0.2f), tint, lerp(tint, Color.Black, 0.2f)))
    }
    val ink = if (asleep) lerp(tint, colors.ink, 0.35f) else Color.White
    Box(
        contentAlignment = Alignment.Center,
        modifier = modifier
            .size(size)
            .shadow(if (asleep) 0.dp else size * 0.08f, shape)
            .background(fill, shape)
            .border(
                BorderStroke(0.75.dp, Brush.linearGradient(listOf(Color.White.copy(alpha = 0.4f), Color.Transparent))),
                shape
            )
    ) {
        if (activity == MarkActivity.WORKING) WorkingLight(colors, size, Modifier.matchParentSize())
        val glyph = kind.glyph()
        if (glyph != null) {
            Icon(painterResource(glyph), contentDescription = null, tint = ink, modifier = Modifier.size(size * 0.6f))
        } else {
            Canvas(Modifier.size(size * 0.6f)) {
                val stroke = this.size.width * 0.14f
                val chevron = Path().apply {
                    moveTo(this@Canvas.size.width * 0.08f, this@Canvas.size.height * 0.24f)
                    lineTo(this@Canvas.size.width * 0.4f, this@Canvas.size.height * 0.5f)
                    lineTo(this@Canvas.size.width * 0.08f, this@Canvas.size.height * 0.76f)
                }
                drawPath(chevron, ink, style = Stroke(stroke, cap = StrokeCap.Round, join = StrokeJoin.Round))
                drawRoundRect(
                    if (asleep) colors.accent.copy(alpha = 0.6f) else lerp(colors.accent, Color.White, 0.25f),
                    topLeft = Offset(this.size.width * 0.58f, this.size.height * 0.2f),
                    size = Size(this.size.width * 0.25f, this.size.height * 0.6f),
                    cornerRadius = CornerRadius(this.size.width * 0.05f)
                )
            }
        }
    }
}

@Composable
private fun WorkingLight(colors: KodosiColors, size: Dp, modifier: Modifier) {
    val turn = signPhase()
    Spacer(
        modifier.drawWithCache {
            val width = (size * 0.07f).toPx().coerceAtLeast(1.5.dp.toPx())
            val light = if (turn == null) {
                SolidColor(colors.glowOrange)
            } else {
                val glow = colors.glowOrange
                Brush.sweepGradient(listOf(Color.Transparent, glow.copy(alpha = 0.2f), glow, Color.Transparent))
            }
            val corner = CornerRadius((size * 0.3f).toPx())
            val outline = Path().apply { addRoundRect(RoundRect(this@drawWithCache.size.toRect(), corner)) }
            val ring = Stroke(width * 2)
            onDrawBehind {
                clipPath(outline) {
                    rotate(360f * (turn?.floatValue ?: 0f)) {
                        drawRoundRect(light, cornerRadius = corner, style = ring)
                    }
                }
            }
        }
    )
}

@Composable
fun StatusSign(form: StatusForm, size: Dp, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val label = stringResource(
        when (form) {
            StatusForm.CHANGED -> R.string.sign_changed
            StatusForm.HAND -> R.string.sign_hand
            StatusForm.QUESTION -> R.string.sign_question
            StatusForm.KEY -> R.string.sign_key
            StatusForm.DONE -> R.string.sign_done
            StatusForm.FAILED -> R.string.sign_failed
        }
    )
    val breath = if (form == StatusForm.CHANGED) signPhase() else null
    Canvas(modifier.size(size + 5.dp).semantics { contentDescription = label }) {
        val unit = size.toPx()
        val origin = Offset((this.size.width - unit) / 2, (this.size.height - unit) / 2)
        fun at(x: Float, y: Float) = Offset(origin.x + x * unit, origin.y + y * unit)
        when (form) {
            StatusForm.CHANGED -> drawCircle(
                colors.accent.copy(alpha = breath?.let { breathAlpha(it.floatValue) } ?: 1f),
                radius = unit * 0.31f,
                center = at(0.5f, 0.5f)
            )

            StatusForm.HAND -> {
                val middle = at(0.5f, 0.56f)
                withTransform({
                    rotate(18f, middle)
                    scale(0.8f, 0.8f, middle)
                }) {
                    for ((index, top) in listOf(0.14f, 0.04f, 0.06f, 0.16f).withIndex()) {
                        drawRoundRect(
                            colors.accent,
                            topLeft = at(0.24f + index * 0.16f, top),
                            size = Size(unit * 0.13f, unit * 0.6f),
                            cornerRadius = CornerRadius(unit * 0.07f)
                        )
                    }
                    drawRoundRect(
                        colors.accent,
                        at(0.24f, 0.42f),
                        Size(unit * 0.61f, unit * 0.54f),
                        CornerRadius(unit * 0.2f)
                    )
                    rotate(-38f, at(0.16f, 0.62f)) {
                        drawRoundRect(
                            colors.accent,
                            at(0.04f, 0.5f),
                            Size(unit * 0.32f, unit * 0.13f),
                            CornerRadius(unit * 0.07f)
                        )
                    }
                }
                val wave = Stroke(unit * 0.08f, cap = StrokeCap.Round)
                val swing = Size(unit * 0.34f, unit * 0.34f)
                drawArc(colors.accent, -75f, 75f, false, at(0.61f, 0.07f), swing, style = wave)
                drawArc(colors.accent, 105f, 75f, false, at(0.03f, 0.61f), swing, style = wave)
            }

            StatusForm.QUESTION -> {
                drawRoundRect(
                    colors.accent,
                    at(0.02f, 0.06f),
                    Size(unit * 0.96f, unit * 0.74f),
                    CornerRadius(unit * 0.26f)
                )
                val tail = Path().apply {
                    moveTo(origin.x + unit * 0.22f, origin.y + unit * 0.7f)
                    lineTo(origin.x + unit * 0.2f, origin.y + unit * 0.98f)
                    lineTo(origin.x + unit * 0.46f, origin.y + unit * 0.74f)
                    close()
                }
                drawPath(tail, colors.accent)
                val mark = Path().apply {
                    moveTo(origin.x + unit * 0.38f, origin.y + unit * 0.32f)
                    cubicTo(
                        origin.x + unit * 0.38f,
                        origin.y + unit * 0.18f,
                        origin.x + unit * 0.62f,
                        origin.y + unit * 0.18f,
                        origin.x + unit * 0.62f,
                        origin.y + unit * 0.32f
                    )
                    cubicTo(
                        origin.x + unit * 0.62f,
                        origin.y + unit * 0.42f,
                        origin.x + unit * 0.5f,
                        origin.y + unit * 0.42f,
                        origin.x + unit * 0.5f,
                        origin.y + unit * 0.52f
                    )
                }
                drawPath(mark, colors.onAccent, style = Stroke(unit * 0.09f, cap = StrokeCap.Round))
                drawCircle(colors.onAccent, radius = unit * 0.055f, center = at(0.5f, 0.66f))
            }

            StatusForm.KEY -> {
                drawCircle(colors.accent, radius = unit * 0.24f, center = at(0.3f, 0.5f), style = Stroke(unit * 0.12f))
                drawRect(colors.accent, at(0.5f, 0.44f), Size(unit * 0.46f, unit * 0.12f))
                drawRect(colors.accent, at(0.78f, 0.5f), Size(unit * 0.1f, unit * 0.2f))
                drawRect(colors.accent, at(0.64f, 0.5f), Size(unit * 0.08f, unit * 0.16f))
            }

            StatusForm.DONE -> {
                val check = Path().apply {
                    moveTo(origin.x + unit * 0.19f, origin.y + unit * 0.53f)
                    lineTo(origin.x + unit * 0.4f, origin.y + unit * 0.73f)
                    lineTo(origin.x + unit * 0.81f, origin.y + unit * 0.28f)
                }
                drawPath(
                    check,
                    colors.ready,
                    style = Stroke(unit * 0.17f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                )
            }

            StatusForm.FAILED -> {
                val triangle = Path().apply {
                    moveTo(origin.x + unit * 0.5f, origin.y + unit * 0.06f)
                    lineTo(origin.x + unit * 0.97f, origin.y + unit * 0.9f)
                    lineTo(origin.x + unit * 0.03f, origin.y + unit * 0.9f)
                    close()
                }
                drawPath(triangle, colors.caution, style = Stroke(unit * 0.1f, join = StrokeJoin.Round))
                drawPath(triangle, colors.caution)
                drawRect(colors.onAccent, at(0.455f, 0.34f), Size(unit * 0.09f, unit * 0.3f))
                drawCircle(colors.onAccent, radius = unit * 0.055f, center = at(0.5f, 0.76f))
            }
        }
    }
}

@Composable
fun Glyph(@DrawableRes id: Int, tint: Color, size: Dp, modifier: Modifier = Modifier, description: String? = null) {
    Icon(painterResource(id), contentDescription = description, tint = tint, modifier = modifier.size(size))
}
