package com.kodosi.mobile.terminals

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.util.Base64
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TerminalKernelTest {
    @Test
    fun aCheckpointRendersAndAcceptsOnlyContiguousOutput() {
        TerminalKernel().use { kernel ->
            install(kernel)
            assertNotNull(kernel.frame())
            assertEquals(
                TerminalKernel.OK,
                kernel.write(direct("\u001b[2J\u001b[H\u001b[31mhello".encodeToByteArray()), 7)
            )
            assertEquals(TerminalKernel.OK, kernel.resize(8, 3, 12, 8, 16))
            assertNotEquals(TerminalKernel.OK, kernel.write(direct("bad".encodeToByteArray()), 9))
            assertEquals(TerminalKernel.OK, kernel.write(direct("!".encodeToByteArray()), 8))
            val frame = requireNotNull(kernel.frame())
            assertEquals("hello!", (0..5).joinToString("") { frame.text(it, 0).orEmpty() })
            assertEquals(12, frame.cols)
            assertEquals(3, frame.rows)
            assertNotEquals(frame.foreground, frame.foreground(0, 0))
        }
    }

    @Test
    fun aVisibleCheckpointRestoresItsImagePixels() {
        TerminalKernel().use { kernel ->
            install(kernel, "visible-image-checkpoint.snapshot", rows = 4, cols = 10)
            assertEquals(TerminalKernel.OK, kernel.resize(7, 4, 10, 8, 16))
            assertNotNull(kernel.frame())
            val images = kernel.images()
            assertTrue(images.isNotEmpty())
            assertTrue(images.any { it.bitmap.getPixel(0, 0) != 0 })
        }
    }

    @Test
    fun rawAndPngKittyImagesKeepTheirAlphaAndColorAcrossJni() {
        val color = Color.argb(128, 64, 32, 16)
        val png = ByteArrayOutputStream().use { stream ->
            Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888).apply {
                setPixel(0, 0, color)
                assertTrue(compress(Bitmap.CompressFormat.PNG, 100, stream))
                recycle()
            }
            stream.toByteArray()
        }
        for ((format, bytes) in listOf(32 to byteArrayOf(64, 32, 16, -128), 100 to png)) {
            TerminalKernel().use { kernel ->
                install(kernel)
                assertEquals(TerminalKernel.OK, kernel.resize(7, 2, 8, 8, 16))
                val encoded = Base64.getEncoder().encodeToString(bytes)
                val show = "\u001b[H\u001b_Ga=T,f=$format,s=1,v=1,i=76,c=1,r=1;$encoded\u001b\\"
                assertEquals(TerminalKernel.OK, kernel.write(direct(show.encodeToByteArray()), 7))
                assertNotNull(kernel.frame())
                val image = kernel.images().single()
                assertEquals("format=$format", color, image.bitmap.getPixel(0, 0))
            }
        }
    }

    @Test
    fun pasteUsesTheProgramsBracketedPasteMode() {
        TerminalKernel().use { kernel ->
            install(kernel)
            assertEquals(TerminalKernel.OK, kernel.write(direct("\u001b[?2004h".encodeToByteArray()), 7))
            assertArrayEquals("\u001b[200~hello\u001b[201~".encodeToByteArray(), kernel.encodePaste("hello"))
        }
    }

    private fun install(
        kernel: TerminalKernel,
        name: String = "visual-checkpoint.snapshot",
        rows: Int = 2,
        cols: Int = 8
    ) {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val checkpoint = assets.open(name).use { it.readBytes() }
        assertEquals(TerminalKernel.OK, kernel.install(direct(checkpoint), 7, rows, cols, false))
    }

    private fun direct(bytes: ByteArray): ByteBuffer = ByteBuffer.allocateDirect(bytes.size).put(bytes).apply { flip() }
}
