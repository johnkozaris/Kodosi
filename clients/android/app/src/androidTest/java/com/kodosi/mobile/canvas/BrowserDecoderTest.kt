package com.kodosi.mobile.canvas

import android.graphics.ImageFormat
import android.hardware.HardwareBuffer
import android.media.ImageReader
import android.os.Handler
import android.os.HandlerThread
import androidx.compose.ui.unit.IntSize
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.util.Base64
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BrowserDecoderTest {
    @Test
    fun aKeyFrameOfALargerPictureReachesTheSurfaceInItsSize() {
        val thread = HandlerThread("Browser pictures").apply { start() }
        val pictures = LinkedBlockingQueue<IntSize>()
        val shown = AtomicReference<IntSize>()
        val reader = ImageReader.newInstance(320, 240, ImageFormat.PRIVATE, 2, HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE)
        val take = ImageReader.OnImageAvailableListener { ready ->
            ready.acquireNextImage()?.use { pictures.add(IntSize(it.cropRect.width(), it.cropRect.height())) }
        }
        reader.setOnImageAvailableListener(take, Handler(thread.looper))
        try {
            BrowserDecoder(reader.surface, shown::set) {}.use { decoder ->
                for ((frame, size) in listOf(SMALLER to IntSize(160, 96), LARGER to IntSize(200, 120))) {
                    assertTrue(decoder.frame(Base64.getDecoder().decode(frame), key = true))
                    assertEquals(size, pictures.poll(5, TimeUnit.SECONDS))
                    assertEquals(size, shown.get())
                }
            }
        } finally {
            reader.close()
            thread.quitSafely()
        }
    }

    private companion object {
        const val SMALLER =
            "AAAAASdCwAyJigiGG8I83AgICB4QCEIwAAAAASjOPIAAAAABJbgAQACX///giigAfHAAEAuOAATn1/ffffffffPBm44Y++aDJf11" +
                "11111110wZ11BlXXXXXXXXXTBnXUGVdddddddddMGddQZV111111110wZ11BlXXXXXXXXXTBnXg="
        const val LARGER =
            "AAAAASdCwAyJigiGiPLLNQICAgeEAhCMAAAAASjOPIAAAAABJbgAQACv//+CKKAfHAAOxwAG++++++++++++uuuuuuuuuuuuuuuu" +
                "uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuvA"
    }
}
