package com.kodosi.mobile.canvas

import com.kodosi.mobile.Preferences
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeJson
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
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CanvasStoreTest {
    private val sent = SentCommands()
    private val store = CanvasStore(sent.sink, TestWords)
    private val item = CanvasItem("file", "report.txt", "text", revision = 1, bytes = 3, changed = 0)
    private val session = RuntimeJson.decodeFromString<RuntimeSession>(
        """{"id":"terminal","incarnationId":"current","kind":"remote","name":"Build",
            "status":"running","connectionState":"connected","working":false}"""
    )

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
    fun aChangedFileCreatesNewsOnceWithoutAnExplicitShow() {
        val terminal = session.copy(canvas = Canvas(listOf(item), change = CanvasOpen(item.key, Long.MAX_VALUE - 1)))

        store.observe(listOf(terminal))
        assertEquals(mapOf("terminal" to item.key), store.news.value)
        store.saw(terminal)
        store.observe(listOf(terminal))
        assertTrue(store.news.value.isEmpty())

        val browser = terminal.copy(
            canvas = terminal.canvas!!.copy(change = CanvasOpen("browser", Long.MAX_VALUE))
        )
        store.observe(listOf(browser))
        assertEquals(mapOf("terminal" to "browser"), store.news.value)
    }

    @Test
    fun explicitOpensReopenTheNewestClosedPartButRespectAChosenPart() {
        val browser = CanvasBrowser("sleeping", 2)
        val canvas = Canvas(listOf(item), browser)
        assertEquals("browser", canvas.openKey(null, setOf("files", "browser")))
        assertEquals("file", canvas.openKey(null, setOf("browser")))
        assertEquals("browser", canvas.openKey("browser", setOf("browser")))
        assertTrue(canvas.openKey("file", setOf("files")) == "file")
    }

    @Test
    fun theFilesPartShowsTheChosenFileElseTheNewestAlsoAfterTheBrowserShowed() {
        val newer = item.copy(key = "newer", name = "notes.txt", changed = 5)
        val canvas = Canvas(listOf(item, newer), CanvasBrowser("running", 9))
        assertEquals(item, canvas.shownFile("file"))
        assertEquals(newer, canvas.shownFile("browser"))
        assertEquals(newer, canvas.shownFile("removed"))
    }

    @Test
    fun closedPartsStayClosedAcrossSnapshotsUntilTheirOwnContentChanges() {
        val browser = CanvasBrowser("sleeping", 2)
        val canvas = Canvas(listOf(item), browser)
        val preferences = Preferences(
            closedCanvas = setOf("terminal/files/0", "terminal/browser/2", "other/files/0")
        )
        assertEquals(setOf("files", "browser"), preferences.closedCanvasParts("terminal", canvas))
        assertEquals(
            setOf("browser"),
            preferences.closedCanvasParts("terminal", canvas.copy(items = listOf(item.copy(changed = 3))))
        )
        assertEquals(
            setOf("files"),
            preferences.closedCanvasParts("terminal", canvas.copy(browser = browser.copy(changed = 3)))
        )
        assertEquals(emptySet<String>(), preferences.closedCanvasParts("unrelated", canvas))
    }

    private fun part(revision: Long, offset: Int, bytes: String, key: String = "file") {
        val encoded = Base64.getEncoder().encodeToString(bytes.encodeToByteArray())
        sent.reply(
            "canvas.request",
            """"answer":{"answer":"part","key":"$key","revision":$revision,"offset":$offset,"total":3,"data":"$encoded"}"""
        )
    }
}
