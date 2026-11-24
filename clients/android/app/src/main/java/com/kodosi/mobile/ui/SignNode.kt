package com.kodosi.mobile.ui

import android.graphics.RenderNode
import android.view.Choreographer
import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas as ComposeCanvas
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.inset
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.coroutines.resume
import kotlin.math.ceil
import kotlin.math.roundToInt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/**
 * A moving sign draws once into a RenderNode of its own, and each step only turns, scales or fades that node.
 * Compose draws the whole window again for any drawing that changes, and its layers do not clip their damage, so
 * the display redraws the whole window for a step of a sign there. The node sits in a frame of the sign's own
 * size, which clips the damage of a step to the sign.
 */
internal class SignNode(private val view: View) {
    val frame = RenderNode("sign frame")
    val node = RenderNode("sign").apply { setHasOverlappingRendering(false) }

    fun turn(degrees: Float) = show(node.setRotationZ(degrees))

    fun fade(alpha: Float) = show(node.setAlpha(alpha))

    fun breathe(scale: Float, alpha: Float) =
        show(node.setScaleX(scale) or node.setScaleY(scale) or node.setAlpha(alpha))

    private fun show(changed: Boolean) {
        if (changed) view.parent?.onDescendantInvalidated(view, view)
    }
}

@Composable
internal fun rememberSignNode(): SignNode {
    val view = LocalView.current
    val sign = remember(view) { SignNode(view) }
    DisposableEffect(sign) {
        onDispose {
            sign.frame.discardDisplayList()
            sign.node.discardDisplayList()
        }
    }
    return sign
}

/**
 * Draws [draw], in this element's size, into [sign]'s node and the node, in its frame, here. The node reaches
 * [reach] past the element on each side, so that a turn keeps the corners that the node's own bounds would cut.
 */
internal fun Modifier.sign(sign: SignNode, reach: Dp = 0.dp, draw: DrawScope.() -> Unit): Modifier = drawWithCache {
    val node = sign.node
    val pad = ceil(reach.toPx()).toInt()
    val width = size.width.roundToInt()
    val height = size.height.roundToInt()
    node.setPosition(-pad, -pad, width + pad, height + pad)
    val recording = node.beginRecording()
    try {
        val area = Size((width + 2 * pad).toFloat(), (height + 2 * pad).toFloat())
        CanvasDrawScope().draw(this, layoutDirection, ComposeCanvas(recording), area) {
            inset(pad.toFloat()) { draw() }
        }
    } finally {
        node.endRecording()
    }
    sign.frame.setPosition(0, 0, width, height)
    sign.frame.beginRecording().apply { drawRenderNode(node) }
    sign.frame.endRecording()
    val bounds = Rect(-pad.toFloat(), -pad.toFloat(), (width + pad).toFloat(), (height + pad).toFloat())
    onDrawBehind {
        drawIntoCanvas { canvas ->
            if (canvas.nativeCanvas.isHardwareAccelerated) {
                canvas.nativeCanvas.drawRenderNode(sign.frame)
            } else {
                // A canvas in memory, such as a test's, cannot draw a RenderNode.
                withTransform({
                    clipRect()
                    rotate(node.rotationZ)
                    scale(node.scaleX, node.scaleY)
                }) {
                    canvas.saveLayer(bounds, Paint().apply { alpha = node.alpha })
                    draw()
                    canvas.restore()
                }
            }
        }
    }
}

// Compose's own dispatcher has no timer: there, each delay wakes a thread of its own.
internal suspend fun onMainLooper(steps: suspend CoroutineScope.() -> Unit) =
    withContext(Dispatchers.Main.immediate, steps)

/**
 * Waits for the next display frame on the main looper and returns its time. A step of a sign then moves a node in
 * that frame; Compose's frame clock would run a frame of its own for each step.
 */
internal suspend fun displayFrameMillis(): Long = suspendCancellableCoroutine { waiting ->
    val choreographer = Choreographer.getInstance()
    val frame = Choreographer.FrameCallback { nanos -> waiting.resume(nanos / 1_000_000) }
    choreographer.postFrameCallback(frame)
    waiting.invokeOnCancellation { choreographer.removeFrameCallback(frame) }
}
