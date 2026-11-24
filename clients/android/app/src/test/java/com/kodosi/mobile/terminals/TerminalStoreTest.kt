package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.string
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class TerminalStoreTest {
    private val sent = SentCommands()
    private val store = TerminalStore(sent.sink)

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
}
