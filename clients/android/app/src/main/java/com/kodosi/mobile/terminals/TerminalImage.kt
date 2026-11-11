package com.kodosi.mobile.terminals

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ColorSpace
import android.graphics.Rect
import android.graphics.RectF

data class TerminalImage(
    val bitmap: Bitmap,
    val layer: Int,
    val z: Int,
    val column: Int,
    val row: Int,
    val offsetX: Int,
    val offsetY: Int,
    val width: Int,
    val height: Int,
    val source: Rect
) {
    fun destination(cellWidth: Int, cellHeight: Int): RectF {
        val x = column.toFloat() * cellWidth + offsetX
        val y = row.toFloat() * cellHeight + offsetY
        return RectF(x, y, x + width, y + height)
    }

    companion object {
        fun decodePng(bytes: ByteArray): Bitmap? {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0 ||
                bounds.outWidth.toLong() * bounds.outHeight * 4 > 64 * 1024 * 1024
            ) {
                return null
            }
            return BitmapFactory.decodeByteArray(
                bytes,
                0,
                bytes.size,
                BitmapFactory.Options().apply {
                    inPreferredConfig = Bitmap.Config.ARGB_8888
                    inPreferredColorSpace = ColorSpace.get(ColorSpace.Named.SRGB)
                    inPremultiplied = false
                    inMutable = true
                    inScaled = false
                }
            )
        }
    }
}
