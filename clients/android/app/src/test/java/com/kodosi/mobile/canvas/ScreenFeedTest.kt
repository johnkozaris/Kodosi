package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import java.util.Base64
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ScreenFeedTest {
    private val sent = SentCommands()
    private val pipes = VideoPipes(sent.sink, Runnable::run)

    @Test
    fun aZoomKeepsThePointUnderTheFingersAndAsksTheComputerForThatPartOfItsSource() = runTest {
        val feed = feed()
        feed.show(true)
        runCurrent()
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        runCurrent()
        news("document.opened", pipe)
        say(
            pipe,
            """{"source":{"kind":"display","id":1},"width":1600.0,"height":1000.0,""" +
                """"access":{"screen":true,"input":true},"control":true}"""
        )

        val fingers = Pair(600f, 120f)
        val before = feed.placement(800f, 500f)?.sourcePoint(fingers.first, fingers.second, 800f, 500f, clamped = false)
        feed.zoom(2f, fingers.first, fingers.second, 800f, 500f)
        val placement = checkNotNull(feed.placement(800f, 500f))
        val after = placement.sourcePoint(fingers.first, fingers.second, 800f, 500f, clamped = false)

        assertEquals(before?.x, after?.x)
        assertEquals(before?.y, after?.y)
        val look = sent()
        assertEquals("look", look.string("type"))
        assertEquals(fields("kind" to "display", "id" to 1), look["source"])
        val region = look["region"]!!.jsonObject
        assertEquals(
            placement.look,
            FrameRegion(
                region["x"]!!.jsonPrimitive.float,
                region["y"]!!.jsonPrimitive.float,
                region["width"]!!.jsonPrimitive.float,
                region["height"]!!.jsonPrimitive.float
            )
        )
    }

    @Test
    fun theScreenSaysTheFirstOfItsStatesThatApplies() {
        fun shows(
            shared: Boolean = true,
            offline: Boolean = false,
            away: ScreenFeed.Away? = null,
            recording: Boolean = true,
            failed: Boolean = false,
            live: Boolean = false,
            kept: Boolean = false
        ) = screenShows(shared, offline, away, recording, failed, live, kept)

        assertEquals(ScreenShows.OFFLINE, shows(shared = false, offline = true, away = ScreenFeed.Away.LOCKED))
        assertEquals(ScreenShows.SHARING_OFF, shows(shared = false, away = ScreenFeed.Away.LOCKED, failed = true))
        assertEquals(ScreenShows.LOCKED, shows(away = ScreenFeed.Away.LOCKED, recording = false, failed = true))
        assertEquals(ScreenShows.ANOTHER_ACCOUNT, shows(away = ScreenFeed.Away.ANOTHER_ACCOUNT, recording = false))
        assertEquals(ScreenShows.ASLEEP, shows(away = ScreenFeed.Away.ASLEEP, recording = false, live = true))
        assertEquals(ScreenShows.UNRECORDED, shows(recording = false, failed = true, live = true))
        assertEquals(ScreenShows.CANT, shows(failed = true, live = true, kept = true))
        assertEquals(ScreenShows.LIVE, shows(live = true, kept = true))
        assertEquals(ScreenShows.RESTING, shows(kept = true))
        assertEquals(ScreenShows.OPENING, shows())
    }

    @Test
    fun aScreenThatItsComputerDoesNotShareHasNoPipeAndOpensOneAsSoonAsItIsShared() = runTest {
        val feed = feed()
        feed.share(false)
        feed.show(true)
        runCurrent()
        assertTrue("document.open" !in sent.types)

        feed.share(true)
        runCurrent()
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        runCurrent()
        feed.share(false)
        assertEquals(pipe, sent.last("document.close").string("pipe"))
    }

    @Test
    fun textCopiedOnTheComputerGoesToThisDevice() = runTest {
        val feed = feed()
        val copied = mutableListOf<String>()
        backgroundScope.launch { feed.copies.collect(copied::add) }
        feed.show(true)
        runCurrent()
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        runCurrent()
        news("document.opened", pipe)
        say(
            pipe,
            """{"source":{"kind":"display","id":1},"width":1600.0,"height":1000.0,""" +
                """"control":true,"copied":"ls -la"}"""
        )
        runCurrent()
        assertEquals(listOf("ls -la"), copied)
    }

    private fun TestScope.feed() =
        ScreenFeed(pipes, "terminal", backgroundScope, listOf(fields("codec" to "h264", "hardware" to true)))
            .also { it.resize(800, 500, 1f) }

    private fun news(type: String, pipe: String) {
        pipes.receive(event(type, """"pipe":"$pipe""""))
    }

    private fun say(pipe: String, message: String) {
        pipes.message(pipe, message.encodeToByteArray())
    }

    private fun sent(): JsonObject {
        val bytes = Base64.getDecoder().decode(sent.last("document.send").string("data"))
        return RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as JsonObject
    }
}
