package com.kodosi.mobile.canvas

import android.graphics.SurfaceTexture
import android.os.Handler
import android.os.Looper
import android.view.Surface
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.util.Base64
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BrowserDecoderTest {
    @Test
    fun rendersAnAnnexBFrameWhenTheViewportHasOddDimensions() {
        val frame = Base64.getDecoder().decode(
            "AAAAAWdCwAraewEQAAADABAAAAMDKPEiagAAAAFozg/IAAABZYiEOiYoAAkC4A=="
        )
        for ((width, height) in listOf(500 to 689, 411 to 900)) {
            val rendered = CountDownLatch(1)
            val texture = SurfaceTexture(false)
            texture.setOnFrameAvailableListener({ rendered.countDown() }, Handler(Looper.getMainLooper()))
            val surface = Surface(texture)
            try {
                BrowserDecoder(surface) {}.use { decoder ->
                    assertTrue(decoder.frame(frame, width, height, key = true))
                    assertTrue(
                        "No browser frame reached the surface for viewport $width x $height",
                        rendered.await(5, TimeUnit.SECONDS)
                    )
                }
            } finally {
                surface.release()
                texture.release()
            }
        }
    }
}
