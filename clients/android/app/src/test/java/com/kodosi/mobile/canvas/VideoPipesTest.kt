package com.kodosi.mobile.canvas

import android.app.Application
import android.graphics.SurfaceTexture
import android.net.wifi.WifiManager
import android.os.Looper
import android.os.PowerManager
import android.view.Surface
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeEventInbox
import com.kodosi.mobile.runtime.RuntimeHandle
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.answer
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.string
import java.util.Base64
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asExecutor
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.testTimeSource
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
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
    private class Sent(val command: JsonObject, val thread: Thread)

    @Test
    fun aFrameIsAcknowledgedWhileTheMainThreadIsBusyAndEveryOtherEventWaitsForIt() {
        val sent = LinkedBlockingQueue<Sent>()
        val commands = CommandSink(TestWords).apply {
            transport = { bytes ->
                val command = RuntimeJson.parseToJsonElement(bytes.decodeToString()).jsonObject
                sent.put(Sent(command, Thread.currentThread()))
                Ffi.OK
            }
        }
        val thread = Executors.newSingleThreadExecutor()
        val pipes = VideoPipes(commands, thread)
        val runtime = RuntimeHandle(pipes::claims)
        val scope = CoroutineScope(SupervisorJob())
        try {
            scope.launch(pipes.dispatcher) { pipes.consume(runtime.videoEvents) }
            BrowserFeed(pipes, "terminal", scope, VideoCodec.decodable).show(true)
            val open = next(sent) { it.string("type") == "document.open" }.command
            commands.receive(answer("document.result", open.string("requestId"), "document.open"))
            val pipe = open.string("pipe")
            val frame = Base64.getEncoder().encodeToString(byteArrayOf(1, 0, 0, 0, 7, 1, 0, 0, 0, 0, 1, 0x65))
            runtime.onEvent(event("document.opened", """"pipe":"$pipe""""))
            runtime.onEvent(event("document.frame", """"pipe":"$pipe","sequence":0,"more":false,"data":"$frame""""))
            runtime.onEvent(event("sessions.snapshot", """"sessions":[]"""))

            val ack = next(sent) { message(it)?.string("type") == "ack" }
            assertEquals(7L, message(ack.command)?.number("frame"))
            assertNotSame(Looper.getMainLooper().thread, ack.thread)
            val waiting = runBlocking { withTimeout(10_000) { runtime.events.next() } }
            assertEquals("sessions.snapshot", (waiting as RuntimeEventInbox.Arrival.Event).event.type)
        } finally {
            scope.cancel()
            thread.shutdown()
        }
    }

    @Test
    fun wifiStaysAwakeWhilePicturesArriveAndSleepsOnceTheyStopOrTheFeedHides() = runTest {
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
            repeat(20) { frame ->
                picture(pipes, pipe, frame)
                advanceTimeBy(1.seconds)
            }
            assertEquals(1, wifi.activeLockCount)
            advanceUntilIdle()
            assertEquals(0, wifi.activeLockCount)

            picture(pipes, pipe, 20)
            assertEquals(1, wifi.activeLockCount)
            feed.show(false)
            assertEquals(0, wifi.activeLockCount)

            // Shown again, the feed decodes on the surface that stayed.
            pipe = open(pipes, feed, sent)
            picture(pipes, pipe, 0)
            assertEquals(1, wifi.activeLockCount)
        } finally {
            feed.close()
            surface.release()
            texture.release()
        }
    }

    @Test
    fun aHotPhoneAsksForPicturesOfHalfItsDensityUntilItCools() = runTest {
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
        assertEquals(3f, scale(sent))
    }

    private fun open(pipes: VideoPipes, feed: VideoFeed, sent: SentCommands): String {
        feed.show(true)
        val pipe = sent.last("document.open").string("pipe")
        sent.reply("document.open")
        pipes.receive(RuntimeEvent.parse(event("document.opened", """"pipe":"$pipe"""")))
        return pipe
    }

    private fun picture(pipes: VideoPipes, pipe: String, sequence: Int) {
        val frame = Base64.getEncoder().encodeToString(byteArrayOf(1, 0, 0, 0, sequence.toByte(), 0, 0, 0))
        val fields = """"pipe":"$pipe","sequence":$sequence,"more":false,"data":"$frame""""
        pipes.receive(RuntimeEvent.parse(event("document.frame", fields)))
    }

    // The scale of the size that the feed last asked for.
    private fun scale(sent: SentCommands): Float? = sent.commands.mapNotNull(::message)
        .last { it.string("type") == "watch" || it.string("type") == "size" }["size"]
        ?.jsonObject?.get("scale")?.jsonPrimitive?.float

    private fun event(type: String, fields: String) =
        """{"type":"$type","accountUserId":null,"accountEpoch":0,$fields}""".encodeToByteArray()

    private fun next(sent: LinkedBlockingQueue<Sent>, wanted: (JsonObject) -> Boolean): Sent {
        while (true) {
            val command = checkNotNull(sent.poll(10, TimeUnit.SECONDS)) { "nothing came from the browser's thread" }
            if (wanted(command.command)) return command
        }
    }

    private fun message(command: JsonObject): JsonObject? {
        if (command.string("type") != "document.send") return null
        val bytes = Base64.getDecoder().decode(command.string("data"))
        return RuntimeJson.parseToJsonElement(bytes.copyOfRange(1, bytes.size).decodeToString()) as? JsonObject
    }
}
