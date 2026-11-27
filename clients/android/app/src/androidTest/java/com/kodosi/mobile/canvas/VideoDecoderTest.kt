package com.kodosi.mobile.canvas

import android.graphics.ImageFormat
import android.hardware.HardwareBuffer
import android.media.ImageReader
import android.os.Handler
import android.os.HandlerThread
import androidx.compose.ui.unit.IntSize
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.nio.ByteBuffer
import java.util.Base64
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class VideoDecoderTest {
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
        val failing = { codec: VideoCodec, size: IntSize ->
            listOf("c2.kodosi.missing.decoder") + deviceDecoders(codec, size)
        }
        VideoDecoder(reader.surface, {}, rendered = {}, refused = {}, lost = {}, decoders = failing).use { decoder ->
            assertEquals(IntSize(160, 96), stream(decoder, SMALLER, IntSize(160, 96)))
        }
    }

    @Test
    fun anHevcKeyframeShowsItsPicture() {
        VideoDecoder(reader.surface, {}, rendered = {}, refused = {}, lost = {}).use { decoder ->
            assertEquals(IntSize(160, 96), stream(decoder, HEVC, IntSize(160, 96), VideoCodec.HEVC))
        }
    }

    @Test
    fun aDecoderThatFailsAtEachTryWithNoOtherLeftRefusesTheStreamOnce() {
        val refusals = LinkedBlockingQueue<VideoDecoder>()
        VideoDecoder(
            reader.surface,
            {},
            rendered = {},
            refused = refusals::add,
            lost = {},
            decoders = { _, _ -> listOf("c2.kodosi.missing.decoder") }
        ).use { decoder ->
            decoder.frame(frame(SMALLER), key = true, VideoCodec.H264)
            assertEquals(decoder, refusals.poll(5, TimeUnit.SECONDS))
            assertNull(refusals.poll(1, TimeUnit.SECONDS))
        }
    }

    // Software decoders (like the emulator's) buffer frames before emitting output, so resend the
    // keyframe until a decoded frame of the expected size arrives.
    private fun stream(
        decoder: VideoDecoder,
        keyframe: String,
        size: IntSize,
        codec: VideoCodec = VideoCodec.H264
    ): IntSize? {
        val end = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (System.nanoTime() < end) {
            decoder.frame(frame(keyframe), key = true, codec)
            if (pictures.poll(100, TimeUnit.MILLISECONDS) == size) return size
        }
        return null
    }

    // A frame in memory outside the Java heap, as the runtime lends it.
    private fun frame(base64: String): ByteBuffer {
        val bytes = Base64.getDecoder().decode(base64)
        return ByteBuffer.allocateDirect(bytes.size).apply {
            put(bytes)
            flip()
        }
    }

    private companion object {
        const val SMALLER =
            "AAAAASdCwAyJigiGG8I83AgICB4QCEIwAAAAASjOPIAAAAABJbgAQACX///giigAfHAAEAuOAATn1/ffffffffPBm44Y++aDJf11" +
                "11111110wZ11BlXXXXXXXXXTBnXUGVdddddddddMGddQZV111111110wZ11BlXXXXXXXXXTBnXg="

        // A 160x96 keyframe from VideoToolbox.
        const val HEVC =
            "AAAAAUABDAP//wFgAAADALAAAAMAAAMAPAAABDAkAAAAAUIBAwFgAAADALAAAAMAAAMAPAAAoBQgYWIBDuRSCC5+E9C+ob1Q" +
                "/qoI9VQT6qoK9VVBfqqqDPVVVBvqqqoO9VVVQf6qqqoEPVVVVSmoCAgIH8IBBAAAAAFEAcBy8FskAAAAASgBrwrbL09AU3/6" +
                "OTvfPh//vg++gnG5/9nP/+6FT+u773+KRsn1Ht/32tg0oLvTTDe2W2A="
    }
}
