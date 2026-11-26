package com.kodosi.mobile.canvas

import android.graphics.BitmapFactory
import android.graphics.PointF
import android.graphics.SurfaceTexture
import android.util.Base64
import android.view.Surface
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import com.kodosi.mobile.runtime.decimal
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.flag
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import kotlin.math.nextDown
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

// The part of a source that a view shows, fit and centred in the view, in the source's points.
data class Placement(val sourceWidth: Float, val sourceHeight: Float, val look: FrameRegion) {
    // View pixels per source point.
    fun fit(viewWidth: Float, viewHeight: Float): Float = minOf(viewWidth / look.width, viewHeight / look.height)

    fun viewRect(region: FrameRegion, viewWidth: Float, viewHeight: Float): FrameRegion {
        val fit = fit(viewWidth, viewHeight)
        return FrameRegion(
            (viewWidth - look.width * fit) / 2 + (region.x - look.x) * fit,
            (viewHeight - look.height * fit) / 2 + (region.y - look.y) * fit,
            region.width * fit,
            region.height * fit
        )
    }

    // The source point under a view point, or null outside the source unless `clamped`.
    fun sourcePoint(x: Float, y: Float, viewWidth: Float, viewHeight: Float, clamped: Boolean): PointF? {
        val fit = fit(viewWidth, viewHeight)
        if (fit <= 0 || sourceWidth <= 0 || sourceHeight <= 0) return null
        var sourceX = look.x + (x - (viewWidth - look.width * fit) / 2) / fit
        var sourceY = look.y + (y - (viewHeight - look.height * fit) / 2) / fit
        if (clamped) {
            sourceX = sourceX.coerceIn(0f, sourceWidth.nextDown())
            sourceY = sourceY.coerceIn(0f, sourceHeight.nextDown())
        }
        if (sourceX !in 0f..<sourceWidth || sourceY !in 0f..<sourceHeight) return null
        return PointF(sourceX, sourceY)
    }
}

// The screen of a terminal's computer on its canvas: the video of the display or window that it
// shows, its pointer, and the zoom of this view. Input and pointer positions are in the shown
// source's points. The zoom and the texture change on the main thread, and the rest on the pipes'
// thread.
class ScreenFeed(pipes: VideoPipes, session: String, scope: CoroutineScope, decoders: List<JsonObject>) :
    VideoFeed(pipes, session, "screen", scope, decoders) {
    data class Source(val kind: String, val id: Long)

    data class Display(val id: Long, val width: Float, val height: Float)

    data class Window(val id: Long, val app: String, val title: String)

    data class Pointer(val x: Float, val y: Float, val shape: String)

    // A pointer's picture with its hotspot and size, in points.
    data class Shape(
        val image: ImageBitmap,
        val hotspotX: Float,
        val hotspotY: Float,
        val width: Float,
        val height: Float
    )

    data class Access(val screen: Boolean, val input: Boolean)

    // Why the computer shows nothing for a while, which ends by itself.
    enum class Away { LOCKED, ASLEEP, ANOTHER_ACCOUNT }

    private data class Size(val width: Float, val height: Float)

    val displays: StateFlow<List<Display>>
        field = MutableStateFlow(emptyList<Display>())
    val windows: StateFlow<List<Window>>
        field = MutableStateFlow(emptyList<Window>())
    val source: StateFlow<Source?>
        field = MutableStateFlow<Source?>(null)
    val pointer: StateFlow<Pointer?>
        field = MutableStateFlow<Pointer?>(null)
    val shapes: StateFlow<Map<String, Shape>>
        field = MutableStateFlow(emptyMap<String, Shape>())
    val access: StateFlow<Access?>
        field = MutableStateFlow<Access?>(null)
    val away: StateFlow<Away?>
        field = MutableStateFlow<Away?>(null)

    // Whether the capture follows this view; another device in control shows what this one shows.
    val control: StateFlow<Boolean>
        field = MutableStateFlow(true)

    // How much closer than the whole source the view shows, from 1.
    val zoom: StateFlow<Float>
        field = MutableStateFlow(1f)

    // Where the source shows in the view, for as long as the source, its size and the zoom stay.
    val placing: StateFlow<Long>
        field = MutableStateFlow(0L)
    private val size = MutableStateFlow<Size?>(null)

    // The source point in the middle of the view while it is zoomed.
    @Volatile
    private var center = PointF()

    @Volatile
    private var view = Size(0f, 0f)

    // The part of the source that the screen captures for this view, or null for all of it.
    private var sentLook: FrameRegion? = null
    private var lookSent = 0L
    private var looking: Job? = null

    // On the main thread.
    private var texture: SurfaceTexture? = null
    private var output: Surface? = null
    private var viewed = false
    private var dropped = false

    // Where the source shows in a view of the given size, or null before the screen said its size.
    // While another device is in control, the view shows what that device shows.
    fun placement(viewWidth: Float, viewHeight: Float): Placement? {
        val size = size.value ?: return null
        if (size.width <= 0 || size.height <= 0 || viewWidth <= 0 || viewHeight <= 0) return null
        if (!control.value) {
            return Placement(size.width, size.height, region.value ?: FrameRegion(0f, 0f, size.width, size.height))
        }
        return own(size, viewWidth, viewHeight)
    }

    // Where the source shows in a view of the given size with this view's own zoom.
    private fun own(size: Size, viewWidth: Float, viewHeight: Float): Placement? {
        if (size.width <= 0 || size.height <= 0 || viewWidth <= 0 || viewHeight <= 0) return null
        val zoom = zoom.value
        if (zoom <= 1f) return Placement(size.width, size.height, FrameRegion(0f, 0f, size.width, size.height))
        val fit = fit(size, viewWidth, viewHeight) * zoom
        val width = minOf(viewWidth / fit, size.width)
        val height = minOf(viewHeight / fit, size.height)
        val center = center
        val look = FrameRegion(
            (center.x - width / 2).coerceIn(0f, size.width - width),
            (center.y - height / 2).coerceIn(0f, size.height - height),
            width,
            height
        )
        return Placement(size.width, size.height, look)
    }

    // Zooms by `factor`, keeping the source point under the view point (x, y) where it is.
    fun zoom(factor: Float, x: Float, y: Float, viewWidth: Float, viewHeight: Float) {
        val size = size.value ?: return
        val held = placement(viewWidth, viewHeight)?.sourcePoint(x, y, viewWidth, viewHeight, clamped = true) ?: return
        val most = maxOf(1f, CLOSEST / fit(size, viewWidth, viewHeight))
        val next = (zoom.value * factor).coerceIn(1f, most)
        if (next == zoom.value) return
        zoom.value = next
        val fit = fit(size, viewWidth, viewHeight) * next
        center = PointF(held.x - (x - viewWidth / 2) / fit, held.y - (y - viewHeight / 2) / fit)
        looked(viewWidth, viewHeight)
    }

    // Moves a zoomed view by (dx, dy) view pixels.
    fun pan(dx: Float, dy: Float, viewWidth: Float, viewHeight: Float) {
        val size = size.value ?: return
        if (zoom.value <= 1f) return
        val fit = fit(size, viewWidth, viewHeight) * zoom.value
        center = PointF(center.x + dx / fit, center.y + dy / fit)
        looked(viewWidth, viewHeight)
    }

    fun showWhole() {
        if (zoom.value <= 1f) return
        zoom.value = 1f
        placing.value++
        execute { sendLook() }
    }

    fun show(source: Source) = execute { post(fields("type" to "show", "source" to source.json)) }

    // Keeps the middle where the source is, so that a pan back starts at once.
    private fun looked(viewWidth: Float, viewHeight: Float) {
        view = Size(viewWidth, viewHeight)
        size.value?.let { own(it, viewWidth, viewHeight)?.look }?.let {
            center = PointF(it.x + it.width / 2, it.y + it.height / 2)
        }
        placing.value++
        execute { sendLook() }
    }

    override fun opened() {
        sentLook = null
    }

    // Without Accessibility, Kodosi Screen cannot play input, and the view still zooms.
    override val takesInput: Boolean get() = access.value?.input != false

    override fun watched() = sendLook()

    override fun settled(width: Int, height: Int) {
        view = Size(width.toFloat(), height.toFloat())
        sendLook()
    }

    // Sends what the view shows at most every LOOK_MS, and the last of a zoom or pan once it ends. Each
    // look names its source, so the screen leaves one that comes after another source took its place.
    private fun sendLook() {
        val source = source.value ?: return
        if (!watching) return
        val look = if (zoom.value > 1f) size.value?.let { own(it, view.width, view.height)?.look } else null
        if (look == sentLook) return
        looking?.cancel()
        val wait = lookSent + LOOK_MS - System.currentTimeMillis()
        if (wait > 0) {
            looking = work.launch {
                delay(wait)
                sendLook()
            }
            return
        }
        sentLook = look
        lookSent = System.currentTimeMillis()
        post(fields("type" to "look", "source" to source.json, "region" to look?.json))
    }

    override fun take(message: JsonObject) {
        (message["sources"] as? JsonObject)?.let { sources ->
            displays.value = sources.objects("displays").mapNotNull { display ->
                val id = display.number("id") ?: return@mapNotNull null
                Display(id, display.decimal("width")?.toFloat() ?: 0f, display.decimal("height")?.toFloat() ?: 0f)
            }
            windows.value = sources.objects("windows").mapNotNull { window ->
                val id = window.number("id") ?: return@mapNotNull null
                Window(id, window.string("app").orEmpty(), window.string("title").orEmpty())
            }
        }
        val shown = (message["source"] as? JsonObject)?.let { shown ->
            val kind = shown.string("kind") ?: return@let null
            shown.number("id")?.let { Source(kind, it) }
        }
        val width = message.decimal("width")?.toFloat() ?: 0f
        val height = message.decimal("height")?.toFloat() ?: 0f
        // The screen shows another source whole to every view.
        if (shown != source.value) {
            source.value = shown
            zoom.value = 1f
            sentLook = null
        }
        val measured = Size(width, height)
        if (measured != size.value) {
            size.value = measured
            placing.value++
        }
        pointer.value = (message["pointer"] as? JsonObject)?.let { at ->
            val shape = (at["shape"] as? JsonPrimitive)?.content ?: return@let null
            Pointer(at.decimal("x")?.toFloat() ?: 0f, at.decimal("y")?.toFloat() ?: 0f, shape)
        }
        (message["access"] as? JsonObject)?.let { access.value = Access(it.flag("screen"), it.flag("input")) }
        away.value = when (message.string("away")) {
            "locked" -> Away.LOCKED
            "asleep" -> Away.ASLEEP
            "anotherAccount" -> Away.ANOTHER_ACCOUNT
            else -> null
        }
        if (away.value != null || access.value?.screen == false) stale()
        control.value = message.flag("control")
        val arrived = message.objects("shapes").mapNotNull(::shape)
        if (arrived.isNotEmpty()) shapes.value = shapes.value + arrived
    }

    // Hands the text that this device pastes to the computer's clipboard, before the keys of the paste.
    fun clipboard(text: String) = execute {
        if (takesInput) post(fields("type" to "clipboard", "text" to text))
    }

    // A view of the feed made this texture, which the feed keeps from now on: the screen's pictures land
    // in it, and its last picture is the still that the next view shows at once.
    internal fun textureMade(made: SurfaceTexture, width: Int, height: Int, density: Float) {
        if (!viewed) release()
        output?.release()
        texture = made
        viewed = true
        output = Surface(made).also { surface(it, width, height, density) }
    }

    // The texture with the last picture, for a view that comes back, unless another view shows it.
    internal fun keptTexture(): SurfaceTexture? = texture?.takeUnless { viewed }?.also { viewed = true }

    internal fun textureLeft(left: SurfaceTexture) {
        if (left !== texture) {
            left.release()
            return
        }
        viewed = false
        if (dropped) release()
    }

    override fun freeStill() {
        if (!viewed) release()
    }

    override fun close() {
        dropped = true
        super.close()
    }

    private fun release() {
        val kept = texture ?: return
        texture = null
        detach()
        output?.release()
        output = null
        kept.release()
    }

    private fun shape(shape: JsonObject): Pair<String, Shape>? {
        val id = (shape["id"] as? JsonPrimitive)?.content ?: return null
        val bytes =
            runCatching { Base64.decode(shape.string("png").orEmpty(), Base64.DEFAULT) }.getOrNull() ?: return null
        val image = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap() ?: return null
        return id to Shape(
            image,
            shape.decimal("x")?.toFloat() ?: 0f,
            shape.decimal("y")?.toFloat() ?: 0f,
            shape.decimal("width")?.toFloat() ?: 0f,
            shape.decimal("height")?.toFloat() ?: 0f
        )
    }

    private fun fit(size: Size, viewWidth: Float, viewHeight: Float) =
        minOf(viewWidth / size.width, viewHeight / size.height)

    private val Source.json: JsonObject get() = fields("kind" to kind, "id" to id)

    private val FrameRegion.json: JsonObject get() = fields("x" to x, "y" to y, "width" to width, "height" to height)

    private companion object {
        // How often a zoom or a pan that goes on moves the capture to what the view shows.
        const val LOOK_MS = 200L

        // The closest view, in view pixels per source point.
        const val CLOSEST = 8f
    }
}
