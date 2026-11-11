package com.kodosi.mobile.ui

import android.graphics.Bitmap
import android.graphics.Matrix
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.draw.innerShadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageShader
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp

enum class Elevation { RESTING, LIFTED, FLOATING }

fun Modifier.raised(
    colors: KodosiColors,
    shape: Shape,
    fill: Color = colors.raised,
    elevation: Elevation = Elevation.RESTING
): Modifier {
    val dark = colors.isDark
    val contact = when (elevation) {
        Elevation.RESTING -> Shadow(
            3.dp,
            colors.shadow.copy(alpha = if (dark) 0.42f else 0.13f),
            offset = DpOffset(1.dp, 2.dp)
        )

        Elevation.LIFTED -> Shadow(
            8.dp,
            colors.shadow.copy(alpha = if (dark) 0.5f else 0.18f),
            offset = DpOffset(2.dp, 5.dp)
        )

        Elevation.FLOATING -> Shadow(
            12.dp,
            colors.shadow.copy(alpha = if (dark) 0.55f else 0.22f),
            offset = DpOffset(3.dp, 8.dp)
        )
    }
    val ambient = when (elevation) {
        Elevation.RESTING -> Shadow(
            6.dp,
            colors.shadow.copy(alpha = if (dark) 0.3f else 0.08f),
            offset = DpOffset(2.dp, 4.dp)
        )

        Elevation.LIFTED -> Shadow(
            13.dp,
            colors.shadow.copy(alpha = if (dark) 0.42f else 0.14f),
            offset = DpOffset(4.dp, 8.dp)
        )

        Elevation.FLOATING -> Shadow(
            22.dp,
            colors.shadow.copy(alpha = if (dark) 0.5f else 0.18f),
            offset = DpOffset(6.dp, 13.dp)
        )
    }
    return dropShadow(shape, ambient)
        .dropShadow(shape, contact)
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
        .border(0.5.dp, colors.hairline.copy(alpha = if (dark) 0.5f else 0.7f), shape)
}

fun Modifier.well(colors: KodosiColors, shape: Shape = RoundedCornerShape(Radius.control)) = this
    .background(colors.well, shape)
    .innerShadow(
        shape,
        Shadow(2.dp, colors.shadow.copy(alpha = if (colors.isDark) 0.5f else 0.1f), offset = DpOffset(0.dp, 1.dp))
    )
    .border(0.5.dp, colors.hairline.copy(alpha = if (colors.isDark) 0.35f else 0.45f), shape)

fun Modifier.bezel(colors: KodosiColors, shape: Shape, fill: Color, quiet: Boolean = false): Modifier {
    val dark = colors.isDark
    return dropShadow(
        shape,
        Shadow(
            if (dark) 1.5.dp else 0.75.dp,
            colors.shadow.copy(
                alpha = if (dark) {
                    if (quiet) 0.21f else 0.35f
                } else {
                    if (quiet) 0.14f else 0.3f
                }
            ),
            offset = DpOffset(0.dp, 1.dp)
        )
    ).background(fill, shape).drawWithCache {
        val outline = shape.createOutline(size, layoutDirection, this)
        val path = Path().apply {
            when (outline) {
                is Outline.Rectangle -> addRect(outline.rect)
                is Outline.Rounded -> addRoundRect(outline.roundRect)
                is Outline.Generic -> addPath(outline.path)
            }
        }
        val distance = 1.dp.toPx()
        val light = Path.combine(
            PathOperation.Difference,
            path,
            Path().apply { addPath(path, Offset(distance, distance)) }
        )
        val shade = Path.combine(
            PathOperation.Difference,
            path,
            Path().apply { addPath(path, Offset(-distance, -distance)) }
        )
        onDrawWithContent {
            drawContent()
            if (dark) drawPath(shade, Color.Black.copy(alpha = 0.2f))
            drawPath(
                light,
                Color.White.copy(
                    alpha = if (dark) {
                        if (quiet) 0.06f else 0.05f
                    } else {
                        if (quiet) 0.35f else 0.22f
                    }
                )
            )
            if (!dark) {
                drawPath(
                    path,
                    if (quiet) colors.shadow.copy(alpha = 0.22f) else Color.Black.copy(alpha = 0.18f),
                    style = Stroke(0.5.dp.toPx())
                )
            }
        }
    }
}

fun Modifier.groundBackdrop(colors: KodosiColors): Modifier = drawWithCache {
    val noise = ImageShader(groundGrain, TileMode.Repeated, TileMode.Repeated).apply {
        setLocalMatrix(Matrix().apply { setScale(density / 2f, density / 2f) })
    }
    val grain = ShaderBrush(noise)
    val light = Brush.radialGradient(
        listOf(colors.accent.copy(alpha = if (colors.isDark) 0.09f else 0.07f), Color.Transparent),
        center = Offset.Zero,
        radius = 620.dp.toPx()
    )
    onDrawBehind {
        drawRect(colors.ground)
        drawRect(light)
        drawRect(
            grain,
            alpha = if (colors.isDark) 0.05f else 0.035f,
            blendMode = if (colors.isDark) BlendMode.Screen else BlendMode.Multiply
        )
    }
}

fun Modifier.terminalTile(colors: KodosiColors, radius: Dp = Radius.tile): Modifier {
    val shape = RoundedCornerShape(radius)
    return dropShadow(
        shape,
        Shadow(10.dp, colors.shadow.copy(alpha = if (colors.isDark) 0.5f else 0.14f), offset = DpOffset(3.dp, 6.dp))
    ).background(colors.terminal, shape)
        .border(0.5.dp, colors.hairline.copy(alpha = 0.7f), shape)
}

private val groundGrain by lazy {
    var seed = 0x9E3779B97F4A7C15uL
    val pixels = IntArray(128 * 128) {
        seed = seed * 6364136223846793005uL + 1442695040888963407uL
        val value = (seed shr 56).toInt()
        0xFF000000.toInt() or (value shl 16) or (value shl 8) or value
    }
    Bitmap.createBitmap(pixels, 128, 128, Bitmap.Config.ARGB_8888).asImageBitmap()
}
