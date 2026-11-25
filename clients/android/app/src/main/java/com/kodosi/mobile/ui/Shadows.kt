package com.kodosi.mobile.ui

import android.graphics.BlurMaskFilter
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.CacheDrawScope
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.PaintingStyle
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.nativePaint
import androidx.compose.ui.graphics.shadow.Shadow
import kotlin.math.abs

// Compose's built-in drop/inner shadow modifiers cache a blurred mask per surface size until
// disposed. Animating surfaces (like a terminal resizing for the IME or a dock thumb) leak a mask
// per frame (~25 MB per keyboard toggle). Drawing via BlurMaskFilter lets the GPU blur each frame
// without caching per-size bitmaps.

fun Modifier.dropShadow(shape: Shape, shadow: Shadow): Modifier = drawWithCache {
    val outline = outlinePath(shape, size)
    val fill = shadowPaint(shadow)
    val edge = shadow.spread.toPx().takeIf { it > 0f }?.let { spread ->
        shadowPaint(shadow).apply {
            style = PaintingStyle.Stroke
            strokeWidth = spread * 2
        }
    }
    val offset = Offset(shadow.offset.x.toPx(), shadow.offset.y.toPx())
    onDrawWithContent {
        translate(offset.x, offset.y) {
            drawIntoCanvas { canvas ->
                canvas.drawPath(outline, fill)
                edge?.let { canvas.drawPath(outline, it) }
            }
        }
        drawContent()
    }
}

fun Modifier.innerShadow(shape: Shape, shadow: Shadow): Modifier = drawWithCache {
    val spread = shadow.spread.toPx()
    val offset = Offset(shadow.offset.x.toPx(), shadow.offset.y.toPx())
    val margin = shadow.radius.toPx() * 3 + abs(offset.x) + abs(offset.y) + spread
    val inner = Size((size.width - spread * 2).coerceAtLeast(0f), (size.height - spread * 2).coerceAtLeast(0f))
    val frame = Path().apply {
        fillType = PathFillType.EvenOdd
        addRect(Rect(-margin, -margin, size.width + margin, size.height + margin))
        addPath(outlinePath(shape, inner), offset + Offset(spread, spread))
    }
    val clip = outlinePath(shape, size)
    val paint = shadowPaint(shadow)
    onDrawWithContent {
        clipPath(clip) { drawIntoCanvas { it.drawPath(frame, paint) } }
        drawContent()
    }
}

private fun CacheDrawScope.outlinePath(shape: Shape, size: Size): Path =
    Path().apply { addOutline(shape.createOutline(size, layoutDirection, this@outlinePath)) }

private fun CacheDrawScope.shadowPaint(shadow: Shadow): Paint = Paint().apply {
    color = shadow.color.copy(alpha = shadow.color.alpha * shadow.alpha)
    blendMode = shadow.blendMode
    val radius = shadow.radius.toPx()
    if (radius > 0f) nativePaint.maskFilter = BlurMaskFilter(radius, BlurMaskFilter.Blur.NORMAL)
}
