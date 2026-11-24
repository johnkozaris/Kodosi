package com.kodosi.mobile.terminals

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.nio.ByteBuffer
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
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
    fun pasteUsesTheProgramsBracketedPasteMode() {
        TerminalKernel().use { kernel ->
            install(kernel)
            assertEquals(TerminalKernel.OK, kernel.write(direct("\u001b[?2004h".encodeToByteArray()), 7))
            assertArrayEquals("\u001b[200~hello\u001b[201~".encodeToByteArray(), kernel.encodePaste("hello"))
        }
    }

    private fun install(kernel: TerminalKernel) {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val checkpoint = assets.open("visual-checkpoint.snapshot").use { it.readBytes() }
        assertEquals(TerminalKernel.OK, kernel.install(direct(checkpoint), 7, 2, 8, false))
    }

    private fun direct(bytes: ByteArray): ByteBuffer = ByteBuffer.allocateDirect(bytes.size).put(bytes).apply { flip() }
}
