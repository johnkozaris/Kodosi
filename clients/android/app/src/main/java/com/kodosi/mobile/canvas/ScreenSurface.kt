package com.kodosi.mobile.canvas

import android.annotation.SuppressLint
import android.content.ClipboardManager
import android.content.Context
import android.graphics.ColorMatrix
import android.graphics.ColorMatrixColorFilter
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.PointF
import android.graphics.SurfaceTexture
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.TextureView
import androidx.compose.ui.unit.IntSize

// A picture that is not the computer's own any more: darker and with less colour.
internal val RESTING = ColorMatrix().apply {
    setSaturation(0.55f)
    postConcat(ColorMatrix().apply { setScale(0.5f, 0.5f, 0.5f, 1f) })
}

@SuppressLint("ViewConstructor")
// A TextureView takes a transform and keeps to the view's bounds, so a zoomed view moves and scales
// the newest picture at once, before the screen sends the part that the view shows. Its texture
// belongs to the feed and outlives the view, so a view that comes back shows the last picture at once.
internal class ScreenSurface(context: Context, private val feed: ScreenFeed) :
    StreamInput(context, feed),
    TextureView.SurfaceTextureListener {
    private val video = TextureView(context).also {
        addView(it, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        it.surfaceTextureListener = this
        feed.keptTexture()?.let(it::setSurfaceTexture)
    }
    private var resting = false
    private var moving: PointF? = null
    private val pinch = ScaleGestureDetector(
        context,
        object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            override fun onScale(detector: ScaleGestureDetector): Boolean {
                feed.zoom(detector.scaleFactor, detector.focusX, detector.focusY, width.toFloat(), height.toFloat())
                return true
            }
        }
    )

    private var placement: Placement? = null
    private var region: FrameRegion? = null
    private var pictureSize: IntSize? = null

    // Moves and scales the picture to where `region` of the source shows, all of it when null; before
    // the screen said its size, the picture fits the view whole.
    fun place(placement: Placement?, region: FrameRegion?, pictureSize: IntSize?) {
        this.placement = placement
        this.region = region
        this.pictureSize = pictureSize
        place()
    }

    private fun place() {
        if (width <= 0 || height <= 0) return
        val viewWidth = width.toFloat()
        val viewHeight = height.toFloat()
        val placement = placement
        val shown = if (placement != null) {
            placement.viewRect(
                region ?: FrameRegion(0f, 0f, placement.sourceWidth, placement.sourceHeight),
                viewWidth,
                viewHeight
            )
        } else {
            val picture = pictureSize ?: return
            val fit = minOf(viewWidth / picture.width, viewHeight / picture.height)
            FrameRegion(
                (viewWidth - picture.width * fit) / 2,
                (viewHeight - picture.height * fit) / 2,
                picture.width * fit,
                picture.height * fit
            )
        }
        video.setTransform(
            Matrix().apply {
                setScale(shown.width / viewWidth, shown.height / viewHeight)
                postTranslate(shown.x, shown.y)
            }
        )
    }

    // Dims the picture while it is not the computer's own.
    fun rest(on: Boolean) {
        if (resting == on) return
        resting = on
        video.setLayerPaint(if (on) Paint().apply { colorFilter = ColorMatrixColorFilter(RESTING) } else null)
    }

    override fun fit(): Float? =
        feed.placement(width.toFloat(), height.toFloat())?.fit(width.toFloat(), height.toFloat())

    override fun paste(): Boolean {
        val clip = context.getSystemService(ClipboardManager::class.java).primaryClip
        val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()
        if (!text.isNullOrEmpty() && text.encodeToByteArray().size <= MOST_PASTED) feed.clipboard(text)
        return true
    }

    override fun streamPoint(x: Float, y: Float, clamped: Boolean): PointF? =
        feed.placement(width.toFloat(), height.toFloat())?.sourcePoint(x, y, width.toFloat(), height.toFloat(), clamped)

    // One finger scrolls the computer; two fingers zoom and move the view.
    override fun scrolls(event: MotionEvent) = event.pointerCount == 1 && !pinch.isInProgress

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.getToolType(0) == MotionEvent.TOOL_TYPE_FINGER) {
            pinch.onTouchEvent(event)
            move(event)
        }
        return super.onTouchEvent(event)
    }

    private fun move(event: MotionEvent) {
        if (event.pointerCount < 2 || event.actionMasked == MotionEvent.ACTION_POINTER_UP) {
            moving = null
            return
        }
        val focus = PointF((event.getX(0) + event.getX(1)) / 2, (event.getY(0) + event.getY(1)) / 2)
        val last = moving
        moving = focus
        if (last != null && event.actionMasked == MotionEvent.ACTION_MOVE) {
            feed.pan(last.x - focus.x, last.y - focus.y, width.toFloat(), height.toFloat())
        }
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        place()
        super.onSizeChanged(w, h, oldw, oldh)
    }

    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) =
        feed.textureMade(texture, this.width, this.height, resources.displayMetrics.density)

    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = Unit

    // The feed keeps the texture and releases it once it has no use for it.
    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
        feed.textureLeft(texture)
        return false
    }

    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) = Unit

    private companion object {
        const val MOST_PASTED = 1024 * 1024
    }
}
