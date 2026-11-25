package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import java.util.Base64
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
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

    private fun TestScope.feed() =
        ScreenFeed(pipes, "terminal", backgroundScope, listOf(fields("codec" to "h264", "hardware" to true)))
            .also { it.resize(800, 500, 1f) }

    private fun news(type: String, pipe: String) {
        pipes.receive(event(type, """"pipe":"$pipe""""))
    }

    private fun say(pipe: String, message: String) {
        val data = Base64.getEncoder().encodeToString(byteArrayOf(0) + message.encodeToByteArray())
        pipes.receive(event("document.frame", """"pipe":"$pipe","sequence":0,"more":false,"data":"$data""""))
    }

    private fun sent(): JsonObject {
        val bytes = Base64.getDecoder().decode(sent.last("document.send").string("data"))
        return RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as JsonObject
    }
}
