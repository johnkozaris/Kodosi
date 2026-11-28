package com.kodosi.mobile.chat

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.ColorPainter
import com.kodosi.mobile.canvas.CanvasStore
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import java.util.Base64
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TerminalChatTest {
    private val sent = SentCommands()
    private val store = CanvasStore(sent.sink, TestWords)
    private val chat = store.chat("terminal")
    private var pipe = ""
    private var sequence = 0

    @Test
    fun aChangeThatWentMissingMakesTheChatAskItsComputerForTheChatAgain() = runTest {
        watch()
        send("""{"snapshot":{"revision":2,"state":"idle","facts":{},"items":[]}}""")
        send(
            """{"update":{"revision":4,"changes":[{"change":"item","item":{"id":"i5","at":5,
              "body":{"kind":"text","text":"Late"}}}]}}"""
        )

        assertTrue(chat.view.value.items.isEmpty())
        assertFalse(chat.view.value.linked)
        assertEquals(listOf("document.close", "document.open"), sent.types.takeLast(2))
        assertEquals(pipe, sent.last("document.close").string("pipe"))
        assertEquals(pipe, sent.last("document.open").string("pipe"))
        opened()
        send(
            """{"snapshot":{"revision":4,"state":"idle","facts":{},"items":[{"id":"i5","at":5,
              "body":{"kind":"text","text":"Late"}}]}}"""
        )
        assertEquals(listOf("i5"), chat.view.value.items.map { it.id })
        assertTrue(chat.view.value.linked)
    }

    @Test
    fun aQuestionThatTheHostStillAsksAfterABreakOfThePipeTakesAnAnswerAgain() = runTest {
        watch()
        val asking =
            """{"snapshot":{"revision":1,"state":"asking","facts":{},"items":[],"asks":[{"id":"a1","item":"i2",
              "body":{"kind":"permission","tool":{"name":"Bash","kind":"command","state":"asking",
                "target":"make test"}}}]}}"""
        send(asking)
        chat.answer(chat.view.value.question!!, ChatAnswer.Allow)
        assertNull(chat.view.value.question)
        store.receive(event("document.lost", """"pipe":"$pipe""""))
        opened()
        send(asking)

        assertEquals("a1", chat.view.value.question?.id)
    }

    @Test
    fun theChatKeepsTheAgentsThatItsAgentReachesAndTheOnesThatAPersonNamedOrHandedOver() = runTest {
        watch()
        send(
            """{"snapshot":{"revision":1,"state":"working","facts":{},
              "peers":[{"terminal":"t2","name":"Timber Elk","agent":"copilot","place":"KodosiWebsite"},
                {"terminal":"t3","name":"Slate Wren","agent":"codex"}],
              "items":[{"id":"i1","at":1,"body":{"kind":"person","text":"Ask @Timber Elk for the schema.",
                  "by":{"id":"u1","name":"Ann"},"mentions":[{"name":"Timber Elk","agent":"copilot"}]}},
                {"id":"i2","at":2,"body":{"kind":"person","text":"","handoff":{"name":"Slate Wren"}}}],
              "queued":[{"id":"q1","text":"","handoff":{"name":"Timber Elk","agent":"copilot"}}]}}"""
        )

        val view = chat.view.value
        assertEquals(
            listOf(
                ChatPeer("t2", "Timber Elk", "copilot", "KodosiWebsite"),
                ChatPeer("t3", "Slate Wren", "codex")
            ),
            view.peers
        )
        assertEquals(listOf(ChatMention("Timber Elk", "copilot")), (view.items[0].body as ChatBody.Person).mentions)
        assertEquals(ChatMention("Slate Wren"), (view.items[1].body as ChatBody.Person).handoff)
        assertEquals(ChatMention("Timber Elk", "copilot"), view.queued.single().handoff)

        send(
            """{"update":{"revision":2,"changes":[
              {"change":"peers","peers":[{"terminal":"t3","name":"Slate Wren","agent":"codex","place":"lab"}]},
              {"change":"reset"}]}}"""
        )
        assertEquals(listOf(ChatPeer("t3", "Slate Wren", "codex", "lab")), chat.view.value.peers)
        assertTrue(chat.view.value.items.isEmpty())
    }

    @Test
    fun aQueuedMessageSaysWhenTheAgentReadsItAndMessagesKeepTheirPictures() = runTest {
        watch()
        send(
            """{"snapshot":{"revision":1,"state":"working","facts":{},
              "items":[{"id":"i1","at":1,"body":{"kind":"person","text":"What breaks here?",
                "pictures":[{"name":"0f3a.png"},{}]}}],
              "queued":[{"id":"q1","text":"And here?","pictures":[{"name":"9c2e.jpg"}],"given":"turn"},
                {"id":"q2","text":"Keep the old error.","given":"step"},
                {"id":"q3","text":"Then deploy."}]}}"""
        )

        val view = chat.view.value
        assertEquals(
            listOf(ChatPicture("0f3a.png"), ChatPicture()),
            (view.items.single().body as ChatBody.Person).pictures
        )
        assertEquals(listOf(ChatGiven.TURN, ChatGiven.STEP, null), view.queued.map { it.given })
        assertEquals(listOf(ChatPicture("9c2e.jpg")), view.queued.first().pictures)
    }

    @Test
    fun picturesGoWithTheNextMessageAndNotWithAnAnswerToAQuestion() = runTest {
        watch()
        send(
            """{"snapshot":{"revision":1,"state":"asking","facts":{},"items":[],"asks":[{"id":"a1",
              "body":{"kind":"question","questions":[{"question":"Which database?","options":[{"label":"Local"}]}]}}]}}"""
        )
        val picture = DraftPicture(byteArrayOf(1, 2, 3), ColorPainter(Color.Black))
        assertTrue(chat.attach(picture))

        assertTrue(chat.submit("The staging one"))
        assertEquals("answer", request().string("do"))
        assertEquals(listOf(picture), chat.pictures.value)

        send(
            """{"update":{"revision":2,"changes":[{"change":"asks","asks":[]},{"change":"state","state":"working"}]}}"""
        )
        assertTrue(chat.submit(""))
        val message = request()
        assertEquals("send", message.string("do"))
        assertEquals("", message.string("text"))
        assertEquals(listOf("AQID"), message.getValue("pictures").jsonArray.map { it.jsonPrimitive.content })
        assertTrue(chat.pictures.value.isEmpty())
    }

    private fun request(): JsonObject = RuntimeJson.parseToJsonElement(
        Base64.getDecoder().decode(sent.last("document.send").string("data")).decodeToString()
    ).jsonObject

    private fun TestScope.watch() {
        backgroundScope.launch { chat.watch() }
        runCurrent()
        pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        runCurrent()
        opened()
    }

    private fun opened() {
        sequence = 0
        store.receive(event("document.opened", """"pipe":"$pipe""""))
    }

    private fun send(frame: String) {
        val compact = RuntimeJson.parseToJsonElement(frame).toString()
        val data = Base64.getEncoder().encodeToString(compact.encodeToByteArray())
        store.receive(
            event("document.frame", """"pipe":"$pipe","sequence":$sequence,"more":false,"data":"$data"""")
        )
        sequence++
    }
}
