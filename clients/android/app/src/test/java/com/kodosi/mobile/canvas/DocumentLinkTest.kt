package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import java.util.Base64
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class DocumentLinkTest {
    private val sent = SentCommands()
    private val link = DocumentLink(sent.sink, "terminal", "file") {}

    @Test
    fun deliversACompleteFrameAndIgnoresTheOldPipeAfterReopening() = runTest {
        val frames = mutableListOf<String>()
        link.onFrame = { frames += it.decodeToString() }
        val first = async(start = CoroutineStart.UNDISPATCHED) { link.open() }
        val old = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        first.await()
        frame(old, 0, true, "a")
        assertEquals(emptyList<String>(), frames)
        frame(old, 1, false, "b")
        assertEquals(listOf("ab"), frames)

        val reopening = async(start = CoroutineStart.UNDISPATCHED) { link.open() }
        val current = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        reopening.await()
        frame(old, 2, false, "old")
        frame(current, 0, false, "new")
        assertEquals(listOf("ab", "new"), frames)
    }

    @Test
    fun aSequenceGapClosesThePipeAndDiscardsItsPartialFrame() = runTest {
        var lost = 0
        val frames = mutableListOf<ByteArray>()
        link.onLost = { lost++ }
        link.onFrame = { frames += it }
        val opening = async(start = CoroutineStart.UNDISPATCHED) { link.open() }
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        opening.await()
        frame(pipe, 0, true, "a")
        frame(pipe, 2, false, "b")
        frame(pipe, 1, false, "late")
        assertEquals(1, lost)
        assertEquals(0, frames.size)
        assertEquals(pipe, sent.last("document.close").string("pipe"))
    }

    private fun frame(pipe: String, sequence: Int, more: Boolean, text: String) {
        val data = Base64.getEncoder().encodeToString(text.encodeToByteArray())
        link.receive(
            event("document.frame", """"pipe":"$pipe","sequence":$sequence,"more":$more,"data":"$data"""")
        )
    }
}
