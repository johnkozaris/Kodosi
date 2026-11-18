package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.InvalidEvent
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.answer
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class TerminalStoreTest {
    private val sent = SentCommands()
    private val store = TerminalStore(sent.sink)

    private fun snapshot(vararg ids: String) = event(
        "sessions.snapshot",
        "\"sessions\":" + ids.joinToString(",", "[", "]") { id ->
            """{"id":"$id","incarnationId":"i-$id","kind":"remote","name":"$id","isOwner":true,"status":"running",""" +
                """"connectionState":"connected","working":false,"attention":false,"hidden":false,"sharedWith":[],""" +
                """"connectedUsers":[],"prompt":false,"repository":false,"hostName":"John's MacBook Pro"}"""
        }
    )

    @Test
    fun readsTheTerminalsOfASnapshot() {
        store.receive(snapshot("a", "b"))

        assertEquals(listOf("a", "b"), store.state.value.sessions.map { it.id })
        assertEquals("John's MacBook Pro", store.session("b")?.hostName)
        assertTrue(store.state.value.loaded)
    }

    @Test
    fun refusesASnapshotThatRepeatsATerminal() {
        assertThrows(InvalidEvent::class.java) { store.receive(snapshot("a", "a")) }
    }

    @Test
    fun knowsTheListOnlyWhenItsOwnRequestIsAnswered() {
        store.list()
        val requestId = sent.last("session.list").string("requestId")

        store.receive(answer("session.result", "another", "session.list"))
        assertFalse(store.state.value.listed)
        store.receive(answer("session.result", requestId, "session.list"))
        assertTrue(store.state.value.listed)
    }

    @Test
    fun streamsOnlyTheTerminalThatShows() {
        store.watch("a", shown = true)
        store.watch("a", shown = true)
        store.watch("b", shown = false)
        store.watch("a", shown = false)

        val visible = sent.commands.filter { it.string("type") == "session.visible" }
        assertEquals(2, visible.size)
        assertEquals(JsonArray(listOf(JsonPrimitive("a"))), visible[0]["sessionIds"])
        assertEquals(JsonArray(emptyList()), visible[1]["sessionIds"])
        assertEquals(JsonPrimitive(false), visible[1]["watching"])
    }

    @Test
    fun aTerminalOpensUntilItsRequestIsAnsweredAndKeepsWhyItDidNot() = runTest {
        val refused = async(start = CoroutineStart.UNDISPATCHED) { store.open("a") }
        assertFalse("a" in store.openResults.value)
        val requestId = sent.last("session.openRemote").string("requestId")
        sent.sink.receive(answer("session.error", requestId, "session.openRemote", "\"message\":\"busy\""))
        assertEquals(OpenResult("busy"), refused.await())
        assertEquals(OpenResult("busy"), store.openResults.value["a"])

        val again = async(start = CoroutineStart.UNDISPATCHED) { store.open("a") }
        assertFalse("a" in store.openResults.value)
        sent.reply("session.openRemote")
        again.await()
        assertEquals(OpenResult(null), store.openResults.value["a"])

        store.disconnect("a")
        assertFalse("a" in store.openResults.value)
    }

    @Test
    fun aRequestThatStopsWaitingEndsTheOpeningUnlessTheTerminalWasLetGo() = runTest {
        launch(start = CoroutineStart.UNDISPATCHED) { store.open("a") }.cancelAndJoin()
        assertEquals(OpenResult(null), store.openResults.value["a"])

        val leaving = launch(start = CoroutineStart.UNDISPATCHED) { store.open("b") }
        store.disconnect("b")
        leaving.cancelAndJoin()
        assertFalse("b" in store.openResults.value)
    }
}
