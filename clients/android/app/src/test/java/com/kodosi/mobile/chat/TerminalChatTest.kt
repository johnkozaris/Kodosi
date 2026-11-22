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
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
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
    fun theChatAppliesEachChangeInOrder() = runTest {
        watch()
        send("""{"snapshot":{"revision":4,"state":"idle","facts":{},"items":[]}}""")
        send(
            """{"update":{"revision":5,"changes":[
              {"change":"item","item":{"id":"i1","at":1,"body":{"kind":"person","text":"Fix the tax in app.py",
                "by":{"id":"user-2","name":"Sam"}}}},
              {"change":"state","state":"working","now":{"kind":"thinking"}}]}}"""
        )
        send(
            """{"update":{"revision":6,"changes":[
              {"change":"item","item":{"id":"i2","at":2,"body":{"kind":"tool","tool":{"name":"Edit","kind":"edit",
                "state":"running","target":"app.py"}}}},
              {"change":"facts","facts":{"model":"sonnet","mode":"edits","context":{"tokens":650,"window":1000},
                "helpers":1}},
              {"change":"state","state":"working","now":{"kind":"tool","item":"i2"}}]}}"""
        )
        send(
            """{"update":{"revision":7,"changes":[{"change":"item","item":{"id":"i3","at":3,
              "body":{"kind":"text","text":"I made","live":true}}}]}}"""
        )
        send("""{"update":{"revision":8,"changes":[{"change":"text","id":"i3","text":" the fix."}]}}""")
        send(
            """{"update":{"revision":9,"changes":[
              {"change":"item","item":{"id":"i2","at":2,"body":{"kind":"tool","tool":{"name":"Edit","kind":"edit",
                "state":"done","target":"app.py","ms":12}}}},
              {"change":"item","item":{"id":"i3","at":3,"body":{"kind":"text","text":"I made the fix."}}},
              {"change":"item","item":{"id":"i4","at":4,"body":{"kind":"turn","end":"done","ms":9017}}},
              {"change":"state","state":"idle"}]}}"""
        )

        val shown = chat.view.value.chat!!
        assertEquals(9L, shown.revision)
        assertEquals(listOf("i1", "i2", "i3", "i4"), shown.items.map { it.id })
        assertEquals(
            ChatBody.Tool(ChatTool("Edit", ChatToolKind.EDIT, ChatToolState.DONE, "app.py", ms = 12)),
            shown.items[1].body
        )
        assertEquals(ChatBody.Text("I made the fix."), shown.items[2].body)
        assertEquals("sonnet", shown.facts.model)
        assertEquals(ChatMode.EDITS, shown.facts.mode)
        assertEquals(ChatState.IDLE, shown.state)
        assertNull(shown.now)
    }

    @Test
    fun afterThePipeOpensAgainTheChatThatTheHostSendsReplacesTheWholeChat() = runTest {
        watch()
        send(
            """{"snapshot":{"revision":2,"state":"working","facts":{"model":"opus"},"items":[
              {"id":"i1","at":1,"body":{"kind":"person","text":"Read notes.md"}}],
              "queued":[{"id":"q1","text":"Then run the tests"}]}}"""
        )
        store.receive(event("document.lost", """"pipe":"$pipe""""))
        assertFalse(chat.view.value.linked)
        opened()
        send("""{"update":{"revision":3,"changes":[{"change":"reset"}]}}""")
        assertEquals(listOf("i1"), chat.view.value.items.map { it.id })
        send(
            """{"snapshot":{"revision":40,"state":"idle","facts":{"model":"sonnet"},"items":[
              {"id":"i9","at":9,"body":{"kind":"text","text":"Done."}}]}}"""
        )

        val shown = chat.view.value
        assertTrue(shown.linked)
        assertEquals(listOf("i9"), shown.items.map { it.id })
        assertTrue(shown.queued.isEmpty())
        assertEquals("sonnet", shown.facts.model)
        assertEquals(ChatState.IDLE, shown.state)
    }

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
    fun theAgentsListOfItsWorkAndTheWaysOnAfterItsPlanFollowTheAgentAndANewConversationHasNone() = runTest {
        watch()
        send(
            """{"snapshot":{"revision":1,"state":"working","facts":{},"items":[],
              "tasks":[{"text":"Adding total discount","state":"working"},
                {"text":"Verifying discounted total","state":"pending"}],
              "asks":[{"id":"a1","body":{"kind":"plan","text":"1. Update total()",
                "choices":["go","alone","leave"]}}]}}"""
        )
        assertEquals(listOf(TaskState.WORKING, TaskState.PENDING), chat.view.value.tasks.map { it.state })
        val plan = chat.view.value.asks.single()
        assertEquals(
            AskBody.Plan("1. Update total()", listOf(PlanChoice.GO, PlanChoice.ALONE, PlanChoice.LEAVE)),
            plan.body
        )
        chat.answer(plan, ChatAnswer.Plan(PlanChoice.ALONE))
        assertEquals(
            listOf(json("""{"do":"answer","ask":"a1","answer":{"kind":"plan","choice":"alone"}}""")),
            requests()
        )
        send(
            """{"update":{"revision":2,"changes":[{"change":"tasks","tasks":[
              {"text":"Adding total discount","state":"done"},
              {"text":"Verifying discounted total","state":"working"}]}]}}"""
        )
        assertEquals(
            listOf(
                ChatTask("Adding total discount", TaskState.DONE),
                ChatTask("Verifying discounted total", TaskState.WORKING)
            ),
            chat.view.value.tasks
        )
        send("""{"update":{"revision":3,"changes":[{"change":"reset"}]}}""")

        assertTrue(chat.view.value.tasks.isEmpty())
        assertTrue(chat.view.value.asks.isEmpty())
    }

    @Test
    fun aMessageTypedWhileAQuestionWaitsAnswersTheQuestionWithItsWordsAndTheFirstAnswerCounts() = runTest {
        watch()
        send(
            """{"snapshot":{"revision":1,"state":"asking","facts":{},"items":[],"asks":[{"id":"a1","item":"i2",
              "body":{"kind":"permission","tool":{"name":"Bash","kind":"command","state":"asking",
                "target":"rm -rf build"},"always":{"kind":"rule","tool":"Bash","text":"rm -rf build"}}}]}}"""
        )
        val ask = chat.view.value.question!!
        assertTrue(chat.submit("  Keep the build folder  "))
        chat.answer(ask, ChatAnswer.Allow)
        assertNull(chat.view.value.question)
        send(
            """{"update":{"revision":2,"changes":[{"change":"asks","asks":[]},
            {"change":"state","state":"working"}]}}"""
        )
        assertTrue(chat.submit("Then run the tests"))
        chat.stop()

        assertTrue(chat.view.value.answers.isEmpty())
        assertEquals(
            listOf(
                json("""{"do":"answer","ask":"a1","answer":{"kind":"deny","text":"Keep the build folder"}}"""),
                json("""{"do":"send","text":"Then run the tests"}"""),
                json("""{"do":"stop"}""")
            ),
            requests()
        )
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

    private fun requests(): List<JsonObject> = sent.commands.filter { it.string("type") == "document.send" }.map {
        json(Base64.getDecoder().decode(it.string("data")).decodeToString())
    }

    private fun json(text: String): JsonObject = RuntimeJson.parseToJsonElement(text).jsonObject
}
