package com.kodosi.mobile.ui

import android.provider.Settings
import androidx.annotation.DrawableRes
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.toRect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.StatusForm

/** True when the person asked the system for less motion. */
@Composable
fun reducedMotion(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
}

enum class Elevation(val depth: Dp) { RESTING(2.dp), LIFTED(6.dp), FLOATING(12.dp) }

/** A raised object: lit from the top left, with a hairline and a shadow that falls down and right. */
fun Modifier.raised(
    colors: KodosiColors,
    shape: Shape,
    fill: Color = colors.raised,
    elevation: Elevation = Elevation.RESTING
) = this
    .shadow(
        elevation.depth,
        shape,
        ambientColor = colors.shadow.copy(alpha = if (colors.isDark) 0.5f else 0.18f),
        spotColor = colors.shadow.copy(alpha = if (colors.isDark) 0.6f else 0.22f)
    )
    .background(fill, shape)
    .border(
        BorderStroke(
            1.dp,
            Brush.linearGradient(
                0f to colors.highlight,
                0.45f to colors.highlight.copy(alpha = 0f),
                0.6f to colors.lowlight.copy(alpha = 0f),
                1f to colors.lowlight
            )
        ),
        shape
    )
    .border(0.5.dp, colors.hairline.copy(alpha = if (colors.isDark) 0.5f else 0.7f), shape)

enum class ButtonKind { PRIMARY, SECONDARY, TINTED, GHOST }

@Composable
fun KodosiButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.PRIMARY,
    large: Boolean = false,
    enabled: Boolean = true
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val shape = RoundedCornerShape(Radius.md)
    val (fill, ink) = when (kind) {
        ButtonKind.PRIMARY -> colors.accent to colors.onAccent
        ButtonKind.SECONDARY -> colors.lifted to colors.ink
        ButtonKind.TINTED -> colors.accentSoft to colors.accent
        ButtonKind.GHOST -> Color.Transparent to colors.accent
    }
    val face = when {
        !enabled && kind != ButtonKind.GHOST -> colors.well
        pressed -> lerp(fill, Color.Black, 0.08f)
        else -> fill
    }
    Box(
        contentAlignment = Alignment.Center,
        modifier = modifier
            .defaultMinSize(minHeight = if (large) 52.dp else 48.dp)
            .then(if (kind == ButtonKind.GHOST) Modifier else Modifier.shadow(1.dp, shape))
            .clip(shape)
            .background(face, shape)
            .clickable(
                interactionSource = interaction,
                indication = null,
                enabled = enabled,
                role = Role.Button,
                onClick = onClick
            )
            .padding(horizontal = 18.dp)
    ) {
        Text(
            text,
            style = if (large) KodosiType.headline else KodosiType.subhead,
            color = if (enabled) ink else colors.inkFaint,
            textAlign = TextAlign.Center
        )
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
        CircularProgressIndicator(color = colors.accent, strokeWidth = 2.dp, modifier = Modifier.size(16.dp))
        Spacer(Modifier.width(10.dp))
        Text(text, style = KodosiType.callout, color = colors.inkMuted)
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

/** The wordmark with the copper cursor that ends it. */
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

private fun AgentKind.tint(account: Int?): Color {
    val shades = when (this) {
        AgentKind.CLAUDE -> listOf(0xFFD97757, 0xFFC4301C, 0xFFD38A0F, 0xFF6B3420, 0xFFC9456B)
        AgentKind.CODEX -> listOf(0xFF5F6B7A, 0xFF2E5A8C, 0xFF2F7D74, 0xFF3A3F4A, 0xFF6E5A7E)
        AgentKind.COPILOT -> listOf(0xFF8660D9, 0xFF4B3BB0, 0xFFB44BC8, 0xFF5A2F6E, 0xFF5B7BE0)
        AgentKind.CURSOR -> listOf(0xFF4E7C8C)
        AgentKind.GEMINI -> listOf(0xFF5579C9)
        AgentKind.OPENCODE -> listOf(0xFF6F6258)
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
    AgentKind.GEMINI -> R.drawable.provider_gemini
    AgentKind.OPENCODE -> R.drawable.provider_opencode
    AgentKind.SHELL -> null
}

/** A squircle in the program's color with its glyph; a warm light circles it while it works. */
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
    val reduced = reducedMotion()
    val turn = if (reduced) {
        0f
    } else {
        rememberInfiniteTransition(label = "working").animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(tween(2400, easing = LinearEasing), RepeatMode.Restart),
            label = "working turn"
        ).value
    }
    Canvas(modifier) {
        val width = (size * 0.07f).toPx().coerceAtLeast(1.5.dp.toPx())
        val brush = if (reduced) {
            Brush.linearGradient(listOf(colors.glowOrange, colors.glowOrange))
        } else {
            Brush.sweepGradient(
                listOf(Color.Transparent, colors.glowOrange.copy(alpha = 0.2f), colors.glowOrange, Color.Transparent)
            )
        }
        val corner = CornerRadius((size * 0.3f).toPx())
        clipPath(Path().apply { addRoundRect(RoundRect(this@Canvas.size.toRect(), corner)) }) {
            rotate(turn) { drawRoundRect(brush, cornerRadius = corner, style = Stroke(width * 2)) }
        }
    }
}

/** The one sign that a terminal waits, finished, failed or changed. */
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
    val breath = if (form == StatusForm.CHANGED && !reducedMotion()) {
        rememberInfiniteTransition(label = "breath").animateFloat(
            initialValue = 0.45f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(1200), RepeatMode.Reverse),
            label = "breath alpha"
        ).value
    } else {
        1f
    }
    Canvas(modifier.size(size + 5.dp).semantics { contentDescription = label }) {
        val unit = size.toPx()
        val origin = Offset((this.size.width - unit) / 2, (this.size.height - unit) / 2)
        fun at(x: Float, y: Float) = Offset(origin.x + x * unit, origin.y + y * unit)
        when (form) {
            StatusForm.CHANGED -> drawCircle(
                colors.accent.copy(alpha = breath),
                radius = unit * 0.31f,
                center = at(0.5f, 0.5f)
            )

            StatusForm.HAND -> {
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

/** A plain glyph from the app's drawables in the color of its text. */
@Composable
fun Glyph(@DrawableRes id: Int, tint: Color, size: Dp, modifier: Modifier = Modifier, description: String? = null) {
    Icon(painterResource(id), contentDescription = description, tint = tint, modifier = modifier.size(size))
}
