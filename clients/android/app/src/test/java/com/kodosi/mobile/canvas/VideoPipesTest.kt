package com.kodosi.mobile.canvas

import android.app.Application
import android.graphics.SurfaceTexture
import android.net.wifi.WifiManager
import android.os.PowerManager
import android.view.Surface
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.VideoRuntime
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import java.nio.ByteBuffer
import java.util.Base64
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.asExecutor
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.testTimeSource
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class VideoPipesTest {
    @Test
    fun wifiWakesForPicturesAndInputAndSleepsAfterAQuietSpellOrOnceTheFeedHides() = runTest {
        val application = RuntimeEnvironment.getApplication()
        val wifi = shadowOf(application.getSystemService(WifiManager::class.java))
        val sent = SentCommands()
        val pipes = VideoPipes(
            sent.sink,
            UnconfinedTestDispatcher(testScheduler).asExecutor(),
            StreamingWifi(application),
            time = testTimeSource
        )
        val feed = BrowserFeed(pipes, "terminal", backgroundScope, VideoCodec.decodable)
        val texture = SurfaceTexture(false)
        val surface = Surface(texture)
        feed.surface(surface, 1080, 1740, 3f)
        try {
            var pipe = open(pipes, feed, sent)
            assertEquals(0, wifi.activeLockCount)
            repeat(20) {
                picture(pipes, pipe)
                advanceTimeBy(1.seconds)
            }
            assertEquals(1, wifi.activeLockCount)
            advanceUntilIdle()
            assertEquals(0, wifi.activeLockCount)
            feed.input(fields("kind" to "text", "text" to "a"))
            assertEquals(1, wifi.activeLockCount)
            advanceUntilIdle()
            assertEquals(0, wifi.activeLockCount)

            picture(pipes, pipe)
            assertEquals(1, wifi.activeLockCount)
            feed.show(false)
            assertEquals(0, wifi.activeLockCount)

            // Shown again, the feed decodes on the surface that stayed.
            pipe = open(pipes, feed, sent)
            picture(pipes, pipe)
            assertEquals(1, wifi.activeLockCount)
        } finally {
            feed.close()
            surface.release()
            texture.release()
        }
    }

    @Test
    fun aHotPhoneAsksForPicturesOfHalfItsDensityUntilItHasClearlyCooled() = runTest {
        val power = RuntimeEnvironment.getApplication().getSystemService(PowerManager::class.java)
        val sent = SentCommands()
        val pipes = VideoPipes(sent.sink, UnconfinedTestDispatcher(testScheduler).asExecutor(), power = power)
        val feed = BrowserFeed(pipes, "terminal", backgroundScope, VideoCodec.decodable)
        feed.resize(1080, 1740, 3f)
        open(pipes, feed, sent)
        assertEquals(3f, scale(sent))

        shadowOf(power).setCurrentThermalStatus(PowerManager.THERMAL_STATUS_SEVERE)
        advanceTimeBy(1.seconds)
        assertEquals(1.5f, scale(sent))
        shadowOf(power).setCurrentThermalStatus(PowerManager.THERMAL_STATUS_MODERATE)
        advanceTimeBy(1.seconds)
        assertEquals(1.5f, scale(sent))
        shadowOf(power).setCurrentThermalStatus(PowerManager.THERMAL_STATUS_LIGHT)
        advanceTimeBy(1.seconds)
        assertEquals(3f, scale(sent))
    }

    @Test
    fun aFeedWatchesEachPipeThatItOpensAsksForAKeyframeOnANewSurfaceAndLetsGoOfThePipeBeforeItCloses() = runTest {
        val sent = SentCommands()
        val pipes = VideoPipes(sent.sink, UnconfinedTestDispatcher(testScheduler).asExecutor())
        val runtime = Watches()
        pipes.runtime = runtime
        val feed = BrowserFeed(pipes, "terminal", backgroundScope, VideoCodec.decodable)
        val texture = SurfaceTexture(false)
        val surface = Surface(texture)
        try {
            feed.show(true)
            val pipe = sent.last("document.open").string("pipe")
            assertEquals(emptyList<String>(), runtime.calls)
            sent.reply("document.open")
            assertEquals(listOf("watch $pipe"), runtime.calls)

            feed.surface(surface, 1080, 1740, 3f)
            assertEquals("restart $pipe", runtime.calls.last())

            runtime.calls.clear()
            pipes.renewPipes()
            assertEquals(listOf("unwatch $pipe", "watch $pipe"), runtime.calls)
            assertEquals(listOf("document.close", "document.open"), sent.types.takeLast(2))

            feed.show(false)
            assertEquals("unwatch $pipe", runtime.calls.last())
            assertEquals(pipe, sent.last("document.close").string("pipe"))
        } finally {
            feed.close()
            surface.release()
            texture.release()
        }
    }

    private class Watches : VideoRuntime {
        val calls = mutableListOf<String>()

        override fun watchVideo(pipe: String) {
            calls += "watch $pipe"
        }

        override fun restartVideo(pipe: String) {
            calls += "restart $pipe"
        }

        override fun unwatchVideo(pipe: String) {
            calls += "unwatch $pipe"
        }
    }

    private fun open(pipes: VideoPipes, feed: VideoFeed, sent: SentCommands): String {
        feed.show(true)
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        pipes.receive(RuntimeEvent.parse(event("document.opened", """"pipe":"$pipe"""")))
        return pipe
    }

    private fun picture(pipes: VideoPipes, pipe: String) {
        pipes.frame(pipe, VideoCodec.H264.wire, key = false, region = null, ByteBuffer.wrap(byteArrayOf(0, 0, 1, 0x21)))
    }

    // The scale of the size that the feed last asked for.
    private fun scale(sent: SentCommands): Float? = sent.commands.mapNotNull(::message)
        .last { it.string("type") == "watch" || it.string("type") == "size" }["size"]
        ?.jsonObject?.get("scale")?.jsonPrimitive?.float

    private fun event(type: String, fields: String) =
        """{"type":"$type","accountUserId":null,"accountEpoch":0,$fields}""".encodeToByteArray()

    private fun message(command: JsonObject): JsonObject? {
        if (command.string("type") != "document.send") return null
        val bytes = Base64.getDecoder().decode(command.string("data"))
        return RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as? JsonObject
    }
}
