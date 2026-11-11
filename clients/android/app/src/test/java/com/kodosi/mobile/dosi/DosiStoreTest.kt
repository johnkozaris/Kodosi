package com.kodosi.mobile.dosi

import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.fields
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Test

class DosiStoreTest {
    private val store = DosiStore(SentCommands().sink)

    @Test
    fun anAnswerReplacesItsQuestionInsteadOfLeavingADuplicateBadge() {
        val question = DosiItem("question", "turn", 0, fields("kind" to "question", "answer" to null))
        snapshot(listOf(question), "one")
        store.see()
        assertEquals(1, store.state.value.badge)
        val answered = question.copy(body = fields("kind" to "question", "answer" to listOf("Yes")))
        store.receive(event("dosi.item", """"item":${RuntimeJson.encodeToString(answered)}"""))
        assertEquals(1, store.state.value.items.size)
        assertEquals(0, store.state.value.badge)
    }

    @Test
    fun aLiveSnapshotKeepsLoadedHistoryButANewConversationDoesNot() {
        val items = (1..4).map { DosiItem("$it", "turn-$it", it.toLong(), fields("kind" to "text")) }
        snapshot(items.take(3), "one")
        snapshot(items.drop(1), "one")
        assertEquals(listOf("1", "2", "3", "4"), store.state.value.items.map { it.id })
        snapshot(listOf(items.last()), "two")
        assertEquals(listOf("4"), store.state.value.items.map { it.id })
    }

    private fun snapshot(items: List<DosiItem>, conversation: String) {
        val state = DosiSnapshot("idle", true, items, conversation = fields("id" to conversation))
        store.receive(event("dosi.snapshot", """"dosi":${RuntimeJson.encodeToString(state)}"""))
    }
}
