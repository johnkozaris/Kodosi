package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import java.util.Base64
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class BrowserFeedTest {
    private val sent = SentCommands()
    private val pipes = BrowserPipes(sent.sink, Runnable::run)

    @Test
    fun aFeedWatchesOnEachOpenOfItsPipeAndShowsALossUntilAFrameComes() = runTest {
        pipes.choose("terminal")
        val feed = feed()
        feed.show(true)
        runCurrent()
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        runCurrent()

        news("document.opened", pipe)
        assertEquals(true, watched()?.get("opened")?.jsonPrimitive?.boolean)
        news("document.lost", pipe, """"message":"Studio is offline."""")
        assertTrue(feed.reconnecting.value)
        assertEquals("Studio is offline.", feed.failure.value)
        news("document.opened", pipe)
        assertEquals(false, watched()?.get("opened")?.jsonPrimitive?.boolean)
        assertNull(feed.failure.value)
        assertTrue(feed.reconnecting.value)
        assertEquals(1, sent.types.count { it == "document.open" })
        feed.show(false)
        assertFalse(feed.reconnecting.value)
    }

    @Test
    fun onlyThePersonsActTakesThePageAndAReturnToTheScreenDoesNot() = runTest {
        pipes.choose("terminal")
        val feed = feed()
        feed.show(true)
        assertEquals(true, look())
        feed.show(false)
        feed.show(true)
        assertEquals(false, look())

        pipes.choose("terminal")
        pipes.forget("terminal")
        feed.show(false)
        feed.show(true)
        assertEquals(false, look())
    }

    @Test
    fun anActOnTheBrowserThatAViewWatchesTakesThePageAtOnceAndOnlyOnce() = runTest {
        val feed = feed()
        feed.show(true)
        assertEquals(false, look())

        pipes.choose("terminal")
        assertEquals(true, watched()?.get("opened")?.jsonPrimitive?.boolean)
        feed.show(false)
        feed.show(true)
        assertEquals(false, look())
    }

    private fun TestScope.feed() =
        BrowserFeed(pipes, "terminal", backgroundScope).also { it.resize(1080, 1740, 2.625f) }

    private fun look(): Boolean? {
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        news("document.opened", pipe)
        return watched()?.get("opened")?.jsonPrimitive?.boolean
    }

    private fun news(type: String, pipe: String, fields: String = "") {
        pipes.receive(event(type, """"pipe":"$pipe"${if (fields.isEmpty()) "" else ",$fields"}"""))
    }

    private fun watched(): JsonObject? {
        val bytes = Base64.getDecoder().decode(sent.last("document.send").string("data"))
        val message = RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as? JsonObject
        return message?.takeIf { it.string("type") == "watch" }
    }
}
