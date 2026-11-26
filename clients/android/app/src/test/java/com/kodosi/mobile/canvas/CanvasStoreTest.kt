package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.answer
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.terminals.RuntimeSession
import java.util.Base64
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CanvasStoreTest {
    private val sent = SentCommands()
    private val store = CanvasStore(sent.sink, TestWords)
    private val item = CanvasItem("file", "report.txt", "text", revision = 1, bytes = 3, changed = 0)

    @Test
    fun restartsAStaleReadWithoutMixingRevisionsAndCachesTheResult() = runTest {
        val reading = async(start = CoroutineStart.UNDISPATCHED) { store.content("terminal", item) }
        part(1, 0, "o")
        runCurrent()
        assertEquals("1", sent.last("canvas.request")["request"]!!.jsonObject["offset"].toString())
        sent.reply("canvas.request", """"answer":{"answer":"stale","revision":2}""")
        runCurrent()
        val restarted = sent.last("canvas.request")["request"]!!.jsonObject
        assertEquals("0", restarted["offset"].toString())
        assertEquals("2", restarted["revision"].toString())
        part(2, 0, "new")
        assertEquals("new", reading.await().decodeToString())
        val count = sent.commands.size
        assertEquals("new", store.content("terminal", item.copy(revision = 2)).decodeToString())
        assertEquals(count, sent.commands.size)
    }

    @Test
    fun rejectsAPartForAnotherFile() = runTest {
        val reading = async(start = CoroutineStart.UNDISPATCHED) {
            runCatching { store.content("terminal", item) }
        }
        part(1, 0, "new", key = "other")
        assertEquals(TestWords.didNotWork, reading.await().exceptionOrNull()?.message)
    }

    @Test
    fun anAccountResetStopsAnUploadBeforeItsNextChunk() = runTest {
        val uploading = async(start = CoroutineStart.UNDISPATCHED) {
            runCatching { store.attach("terminal", "report", ByteArray(CanvasStore.PART_BYTES + 1)) }
        }
        store.reset()
        sent.reply("canvas.request", """"answer":{"answer":"attached","key":null}""")
        assertEquals(TestWords.accountChanged, uploading.await().exceptionOrNull()?.message)
        assertEquals(1, sent.commands.size)
    }

    @Test
    fun afterTheAppFallsBehindAnOpenPipeAndAPipeWhoseAnswerWentMissingBothStartAgain() = runTest {
        val open = store.link("terminal", "browser")
        val opening = async(start = CoroutineStart.UNDISPATCHED) { open.open() }
        val first = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        opening.await()
        store.receive(event("document.opened", """"pipe":"$first""""))
        val waiting = store.link("other", "browser")
        val registering = async(start = CoroutineStart.UNDISPATCHED) { waiting.open() }
        val second = sent.last("document.open").string("pipe")
        val unanswered = sent.last("document.open").string("requestId")
        sent.commands.clear()

        sent.sink.abandonPending()
        store.renewPipes()
        runCurrent()

        for (pipe in listOf(first, second)) {
            assertEquals(
                listOf("document.close", "document.open"),
                sent.commands.filter { it.string("pipe") == pipe }.map { it.string("type") }
            )
        }
        val late = answer("document.result", unanswered, "document.open")
        assertEquals(CommandSink.Delivery.CALLER, sent.sink.receive(late))
        sent.reply("document.open")
        registering.await()
    }

    @Test
    fun anAsleepBrowserIsWokenOnceEachTimeItFallsAsleep() = runTest {
        store.observe(listOf(terminal(browser = "asleep")))
        val waking = async(start = CoroutineStart.UNDISPATCHED) { store.wakeOnce("terminal") }
        store.wakeOnce("terminal")
        sent.reply("canvas.request", """"answer":{"answer":"done"}""")
        waking.await()
        assertEquals(1, wakes())

        store.observe(listOf(terminal(browser = "awake")))
        store.observe(listOf(terminal(browser = "asleep")))
        val again = async(start = CoroutineStart.UNDISPATCHED) { store.wakeOnce("terminal") }
        sent.reply("canvas.request", """"answer":{"answer":"done"}""")
        again.await()
        assertEquals(2, wakes())
    }

    private fun terminal(browser: String) = RuntimeSession(
        id = "terminal",
        incarnationId = "019363fc-6fca-7000-8000-000000000002",
        name = "Review",
        kind = RuntimeSession.Kind.REMOTE,
        status = RuntimeSession.Status.RUNNING,
        connectionState = RuntimeSession.Connection.CONNECTED,
        isWorking = false,
        canvas = Canvas(browser = CanvasBrowser(browser, changed = 0))
    )

    private fun wakes() = sent.commands.count {
        it.string("type") == "canvas.request" && it["request"]!!.jsonObject.string("type") == "act"
    }

    private fun part(revision: Long, offset: Int, bytes: String, key: String = "file") {
        val encoded = Base64.getEncoder().encodeToString(bytes.encodeToByteArray())
        sent.reply(
            "canvas.request",
            """"answer":{"answer":"part","key":"$key","revision":$revision,"offset":$offset,"total":3,"data":"$encoded"}"""
        )
    }
}
