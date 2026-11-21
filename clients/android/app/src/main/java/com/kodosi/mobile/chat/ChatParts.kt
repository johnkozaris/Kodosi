package com.kodosi.mobile.chat

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.StatusForm
import com.kodosi.mobile.ui.DarkColors
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.LocalPowerSaving
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.snappy
import com.kodosi.mobile.ui.terminalTile
import kotlinx.coroutines.delay

internal object ChatType {
    private val terminal = FontFamily(Font(R.font.jetbrains_mono_regular))
    val mono = TextStyle(fontFamily = terminal, fontSize = 12.sp, lineHeight = 17.sp)
    val monoCaption = TextStyle(fontFamily = terminal, fontSize = 11.sp, lineHeight = 15.sp)
}

/** Code and what a command printed stay dark in both appearances, as terminals do. */
@Composable
internal fun TerminalTile(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    Box(modifier.terminalTile(DarkColors, TileRadius).padding(horizontal = 10.dp, vertical = 8.dp), content = content)
}

internal val TileRadius = 10.dp

@Composable
internal fun DayCapsule(text: String, modifier: Modifier = Modifier) {
    Box(modifier, contentAlignment = Alignment.Center) {
        Capsule(text, Modifier.semantics { heading() })
    }
}

@Composable
internal fun Capsule(text: String, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    Box(
        modifier.defaultMinSize(minHeight = 22.dp)
            .background(lerp(colors.surface, colors.ink, 0.06f), RoundedCornerShape(percent = 50))
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(text, style = KodosiType.caption, color = colors.inkFaint, maxLines = 1)
    }
}

@Composable
internal fun RollingText(
    text: String,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    maxLines: Int = 1,
    overflow: TextOverflow = TextOverflow.Ellipsis
) {
    val sliding = snappy<IntOffset>()
    val fading = fade<Float>()
    val sizing = snappy<IntSize>()
    AnimatedContent(
        text,
        modifier.clipToBounds(),
        transitionSpec = {
            (slideInVertically(sliding) { it } + fadeIn(fading)) togetherWith
                (slideOutVertically(sliding) { -it } + fadeOut(fading)) using
                SizeTransform(clip = true) { _, _ -> sizing }
        },
        label = "rolling text"
    ) { shown -> Text(shown, style = style, color = color, maxLines = maxLines, overflow = overflow) }
}

@Composable
internal fun PoppingSign(form: StatusForm?, size: Dp, modifier: Modifier = Modifier) {
    val fading = fade<Float>()
    val popping = snappy<Float>()
    val sizing = snappy<IntSize>()
    AnimatedContent(
        form,
        modifier,
        transitionSpec = {
            (fadeIn(fading) + scaleIn(popping, initialScale = 0.6f)) togetherWith
                (fadeOut(fading) + scaleOut(popping, targetScale = 0.6f)) using
                SizeTransform(clip = false) { _, _ -> sizing }
        },
        label = "sign"
    ) { shown -> if (shown != null) StatusSign(shown, size) }
}

@Composable
internal fun TypingCursor(modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val still = reducedMotion() || LocalPowerSaving.current
    val shown = remember { Animatable(1f) }
    LaunchedEffect(still) {
        shown.snapTo(1f)
        if (still) return@LaunchedEffect
        delay(1100)
        while (true) {
            shown.animateTo(0f, tween(80))
            delay(360)
            shown.animateTo(1f, tween(80))
            delay(480)
        }
    }
    Box(modifier.graphicsLayer { alpha = shown.value }.background(colors.accent, RoundedCornerShape(1.dp)))
}

@Composable
internal fun Rising(fresh: Boolean, modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    val reduced = reducedMotion()
    val progress = remember { Animatable(if (fresh && !reduced) 0f else 1f) }
    val rising = snappy<Float>()
    LaunchedEffect(progress) { if (progress.value < 1f) progress.animateTo(1f, rising) }
    Box(
        modifier.graphicsLayer {
            alpha = progress.value.coerceIn(0f, 1f)
            translationY = (1f - progress.value) * 8.dp.toPx()
        },
        content = content
    )
}

@Composable
internal fun WavingHand(cue: Any, size: Dp, modifier: Modifier = Modifier) {
    val reduced = reducedMotion()
    val turn = remember { Animatable(0f) }
    LaunchedEffect(cue, reduced) {
        if (reduced) return@LaunchedEffect
        turn.snapTo(0f)
        turn.animateTo(
            0f,
            keyframes {
                durationMillis = 720
                -16f at 120
                14f at 280
                -10f at 430
                6f at 570
            }
        )
    }
    StatusSign(
        StatusForm.HAND,
        size,
        modifier.graphicsLayer {
            rotationZ = turn.value
            transformOrigin = TransformOrigin(0.5f, 0.9f)
        }
    )
}
