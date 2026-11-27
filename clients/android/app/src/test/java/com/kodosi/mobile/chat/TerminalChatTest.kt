package com.kodosi.mobile.chat

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
