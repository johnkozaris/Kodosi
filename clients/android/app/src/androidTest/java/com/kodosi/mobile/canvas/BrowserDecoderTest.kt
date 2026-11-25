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
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BrowserDecoderTest {
    private val thread = HandlerThread("Browser pictures").apply { start() }
    private val pictures = LinkedBlockingQueue<IntSize>()
    private val reader =
        ImageReader.newInstance(320, 240, ImageFormat.PRIVATE, 2, HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE).apply {
            val take = ImageReader.OnImageAvailableListener { ready ->
                ready.acquireNextImage()?.use { pictures.add(IntSize(it.cropRect.width(), it.cropRect.height())) }
            }
            setOnImageAvailableListener(take, Handler(thread.looper))
        }

    @After
    fun close() {
        reader.close()
        thread.quitSafely()
    }

    @Test
    fun aDecoderThatFailsAtEachTryMakesWayForTheNextOne() {
        val failing = { size: IntSize -> listOf("c2.kodosi.missing.decoder") + deviceDecoders(size) }
        BrowserDecoder(reader.surface, {}, refused = {}, lost = {}, decoders = failing).use { decoder ->
            assertEquals(IntSize(160, 96), stream(decoder, SMALLER, IntSize(160, 96)))
        }
    }

    @Test
    fun aDecoderThatFailsAtEachTryWithNoOtherLeftRefusesTheStreamOnce() {
        val refusals = LinkedBlockingQueue<BrowserDecoder>()
        BrowserDecoder(reader.surface, {}, refusals::add, lost = {}, decoders = { listOf("c2.kodosi.missing.decoder") })
            .use { decoder ->
                assertTrue(decoder.frame(Base64.getDecoder().decode(SMALLER), key = true))
                assertEquals(decoder, refusals.poll(5, TimeUnit.SECONDS))
                assertNull(refusals.poll(1, TimeUnit.SECONDS))
            }
    }

    // Software decoders (like the emulator's) buffer frames before emitting output, so resend the
    // keyframe until a decoded frame of the expected size arrives.
    private fun stream(decoder: BrowserDecoder, frame: String, size: IntSize): IntSize? {
        val bytes = Base64.getDecoder().decode(frame)
        val end = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (System.nanoTime() < end) {
            decoder.frame(bytes, key = true)
            if (pictures.poll(100, TimeUnit.MILLISECONDS) == size) return size
        }
        return null
    }

    private companion object {
        const val SMALLER =
            "AAAAASdCwAyJigiGG8I83AgICB4QCEIwAAAAASjOPIAAAAABJbgAQACX///giigAfHAAEAuOAATn1/ffffffffPBm44Y++aDJf11" +
                "11111110wZ11BlXXXXXXXXXTBnXUGVdddddddddMGddQZV111111110wZ11BlXXXXXXXXXTBnXg="
    }
}
