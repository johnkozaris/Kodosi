package com.kodosi.mobile.canvas

import android.app.Application
import android.os.Looper
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeEventInbox
import com.kodosi.mobile.runtime.RuntimeHandle
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.answer
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.string
import java.util.Base64
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

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
