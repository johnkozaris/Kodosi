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
    fun deliversACompleteFrameOnceThePipeOpensAndIgnoresTheOldPipeAfterAnotherOpen() = runTest {
        val frames = mutableListOf<String>()
        link.onFrame = { frames += it.decodeToString() }
        val old = open()
        frame(old, 0, false, "early")
        opened(old)
        frame(old, 0, true, "a")
        assertEquals(emptyList<String>(), frames)
        frame(old, 1, false, "b")
        assertEquals(listOf("ab"), frames)

        val current = open()
        opened(current)
        frame(old, 2, false, "old")
        frame(current, 0, false, "new")
        assertEquals(listOf("ab", "new"), frames)
    }

    @Test
    fun aSequenceGapOpensThePipeAgainAndThePartsOnTheirWayGoNowhere() = runTest {
        val news = mutableListOf<DocumentLink.News>()
        val frames = mutableListOf<String>()
        link.onNews = { news += it }
        link.onFrame = { frames += it.decodeToString() }
        val pipe = open()
        opened(pipe)
        frame(pipe, 0, true, "a")
        frame(pipe, 2, false, "b")
        assertEquals(listOf("document.close", "document.open"), sent.types.takeLast(2))
        assertEquals(pipe, sent.last("document.close").string("pipe"))
        assertEquals(pipe, sent.last("document.open").string("pipe"))
        frame(pipe, 3, false, "late")
        opened(pipe)
        frame(pipe, 0, false, "again")
        assertEquals(listOf("again"), frames)
        assertEquals(listOf(DocumentLink.News.Open, DocumentLink.News.Open), news)
    }

    private suspend fun kotlinx.coroutines.test.TestScope.open(): String {
        val opening = async(start = CoroutineStart.UNDISPATCHED) { link.open() }
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        opening.await()
        return pipe
    }

    private fun opened(pipe: String) {
        link.receive(event("document.opened", """"pipe":"$pipe""""))
    }

    private fun frame(pipe: String, sequence: Int, more: Boolean, text: String) {
        val data = Base64.getEncoder().encodeToString(text.encodeToByteArray())
        link.receive(
            event("document.frame", """"pipe":"$pipe","sequence":$sequence,"more":$more,"data":"$data"""")
        )
    }
}
