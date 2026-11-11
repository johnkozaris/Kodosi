package com.kodosi.mobile.canvas

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateOffsetAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.GenericShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.personTint
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.tint
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

@Composable
internal fun BrowserPointers(window: JsonObject, browser: CanvasBrowser, feed: BrowserFeed) {
    BoxWithConstraints(Modifier.fillMaxSize().clearAndSetSemantics {}) {
        val width = window.number("width")?.toFloat() ?: return@BoxWithConstraints
        val height = window.number("height")?.toFloat() ?: return@BoxWithConstraints
        if (width <= 0 || height <= 0) return@BoxWithConstraints
        val fit = minOf(maxWidth.value / width, maxHeight.value / height)
        val left = (maxWidth.value - width * fit) / 2
        val top = (maxHeight.value - height * fit) / 2
        val project: (Float, Float) -> Offset = { x, y -> Offset(left + x * fit, top + y * fit) }
        window.objects("pointers").forEachIndexed { index, pointer ->
            val x = (pointer["x"] as? JsonPrimitive)?.content?.toFloatOrNull() ?: return@forEachIndexed
            val y = (pointer["y"] as? JsonPrimitive)?.content?.toFloatOrNull() ?: return@forEachIndexed
            if (!x.isFinite() || !y.isFinite()) return@forEachIndexed
            key(pointer.string("viewer"), index) {
                HumanPointer(
                    project(x, y),
                    pointer.string("name").orEmpty().take(80),
                    personTint(pointer.string("person").orEmpty())
                )
            }
        }
        AgentPointer(browser, feed, project)
    }
}

@Composable
private fun HumanPointer(target: Offset, name: String, tint: Color) {
    val at by animateOffsetAsState(
        target,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "person pointer"
    )
    Box(Modifier.offset { IntOffset(at.x.dp.roundToPx(), at.y.dp.roundToPx()) }) {
        PointerArrow(tint, false)
        if (name.isNotBlank()) {
            Text(
                name,
                Modifier.offset(12.dp, 18.dp)
                    .background(tint, CircleShape).padding(horizontal = 5.dp, vertical = 1.dp),
                style = KodosiType.caption,
                color = Color.White,
                maxLines = 1
            )
        }
    }
}

@Composable
private fun AgentPointer(browser: CanvasBrowser, feed: BrowserFeed, project: (Float, Float) -> Offset) {
    val touches by feed.touches.collectAsStateWithLifecycle()
    val address = browser.tabs.firstOrNull { it.active }?.url
    var lastTouch by remember { mutableLongStateOf(touches) }
    var lastAddress by remember { mutableStateOf(address) }
    var shown by remember { mutableStateOf<JsonObject?>(null) }
    var clicks by remember { mutableIntStateOf(0) }
    LaunchedEffect(browser.doing, touches, address) {
        if (touches != lastTouch || address != lastAddress) {
            shown = null
        } else {
            browser.doing?.takeIf { it["point"] is JsonObject }?.let {
                if (it != shown) {
                    shown = it
                    if (it.string("kind") == "click") clicks++
                }
            }
        }
        lastTouch = touches
        lastAddress = address
        if (browser.doing == null) {
            delay(1000)
            shown = null
        }
    }
    val action = shown ?: return
    val point = action["point"] as? JsonObject ?: return
    val x = point.number("x")?.toFloat() ?: return
    val y = point.number("y")?.toFloat() ?: return
    val reduced = reducedMotion()
    val at by animateOffsetAsState(
        project(x, y),
        if (reduced) snap() else spring(dampingRatio = 0.875f, stiffness = 420f),
        label = "agent pointer"
    )
    val kind = AgentKind.named(action.string("agent"))
    val tint = kind.tint()
    val ring = remember { Animatable(1f) }
    LaunchedEffect(clicks, reduced) {
        if (clicks > 0 && !reduced) {
            ring.snapTo(0f)
            delay(240)
            ring.animateTo(1f, tween(420))
        } else {
            ring.snapTo(1f)
        }
    }
    Box(Modifier.offset { IntOffset(at.x.dp.roundToPx(), at.y.dp.roundToPx()) }) {
        if (!reduced) {
            Canvas(Modifier.offset((-17).dp, (-17).dp).size(34.dp)) {
                drawCircle(
                    tint.copy(alpha = 0.9f * (1f - ring.value)),
                    radius = size.width / 2 * (0.3f + 0.95f * ring.value),
                    style = Stroke(2.dp.toPx())
                )
            }
        }
        PointerArrow(tint, browser.doing?.string("kind") == "click")
        AgentMark(
            kind,
            18.dp,
            Modifier.offset(13.dp, 17.dp),
            activity = if (browser.doing == null) MarkActivity.AWAKE else MarkActivity.WORKING
        )
    }
}

private fun pointerPath(width: Float, height: Float): Path = Path().apply {
    val x = width / 15
    val y = height / 21
    moveTo(0f, 0f)
    lineTo(0f, 17 * y)
    lineTo(4.3f * x, 13.2f * y)
    lineTo(7.4f * x, 20 * y)
    lineTo(10.4f * x, 18.7f * y)
    lineTo(7.4f * x, 12.1f * y)
    lineTo(13 * x, 12.1f * y)
    close()
}

private val pointerShape: Shape = GenericShape { size, _ -> addPath(pointerPath(size.width, size.height)) }

@Composable
private fun PointerArrow(tint: Color, pressed: Boolean) {
    Box(
        Modifier.size(15.dp, 21.dp).graphicsLayer {
            scaleX = if (pressed) 0.86f else 1f
            scaleY = scaleX
            transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0f, 0f)
        }.dropShadow(pointerShape, Shadow(1.5.dp, Color.Black.copy(alpha = 0.35f), offset = DpOffset(0.6.dp, 1.2.dp)))
            .drawWithCache {
                val path = pointerPath(size.width, size.height)
                onDrawBehind {
                    drawPath(path, tint)
                    drawPath(path, Color.White, style = Stroke(1.4.dp.toPx(), join = StrokeJoin.Round))
                }
            }
    )
}
