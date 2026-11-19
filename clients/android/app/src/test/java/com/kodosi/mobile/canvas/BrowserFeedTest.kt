package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.answer
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.terminals.RuntimeSession
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class BrowserFeedTest {
    private val sent = SentCommands()
    private val store = CanvasStore(sent.sink, TestWords)
    private val away = RuntimeJson.decodeFromString<RuntimeSession>(
        """{"id":"terminal","incarnationId":"current","kind":"remote","name":"Build",
            "status":"reconnecting","connectionState":"connected","working":false}"""
    )

    @Test
    fun aFeedOpensItsPipeAgainAsSoonAsItsTerminalIsBack() = runTest {
        val feed = BrowserFeed(store, "terminal", backgroundScope)
        store.observe(listOf(away))
        feed.show(true)
        runCurrent()
        refuse()
        advanceTimeBy(1_000)
        runCurrent()
        refuse()
        assertEquals(2, opens())

        store.observe(listOf(away.copy(status = RuntimeSession.Status.RUNNING)))
        runCurrent()
        assertEquals(3, opens())
        assertEquals(1_000, currentTime)
    }

    private fun opens() = sent.types.count { it == "document.open" }

    private fun kotlinx.coroutines.test.TestScope.refuse() {
        sent.sink.receive(answer("document.error", sent.last("document.open").string("requestId"), "document.open"))
        runCurrent()
    }
}
