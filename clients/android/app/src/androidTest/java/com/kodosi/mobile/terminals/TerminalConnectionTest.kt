package com.kodosi.mobile.terminals

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.kodosi.mobile.AndroidWords
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.TerminalLink
import com.kodosi.mobile.runtime.TerminalRuntime
import java.nio.ByteBuffer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TerminalConnectionTest {
    @Test
    fun inputWaitsForItsCheckpointAndConnectResultAndKeepsItsOrder() = runBlocking {
        Fixture().use { fixture ->
            val link = withTimeout(5_000) { fixture.runtime.connections.receive() }
            assertTrue(fixture.connection.send("first".encodeToByteArray()))
            assertTrue(fixture.connection.send("second".encodeToByteArray()))
            assertTrue(fixture.runtime.inputs.tryReceive().isFailure)

            assertEquals(Ffi.OK, install(link))
            assertEquals(TerminalConnection.State.Connecting, fixture.connection.state.value)
            fixture.runtime.succeed(link)
            assertArrayEquals("first".encodeToByteArray(), withTimeout(5_000) { fixture.runtime.inputs.receive() })
            assertArrayEquals("second".encodeToByteArray(), withTimeout(5_000) { fixture.runtime.inputs.receive() })
            assertTrue(fixture.connection.send("third".encodeToByteArray()))
            assertArrayEquals("third".encodeToByteArray(), withTimeout(5_000) { fixture.runtime.inputs.receive() })
        }
    }

    @Test
    fun closingDiscardsUnsentInputAndRejectsALateCheckpoint() = runBlocking {
        for (checkpointInstalled in listOf(false, true)) {
            Fixture().use { fixture ->
                val link = withTimeout(5_000) { fixture.runtime.connections.receive() }
                assertTrue(fixture.connection.send("never".encodeToByteArray()))
                if (checkpointInstalled) assertEquals(Ffi.OK, install(link))
                fixture.connection.close()

                fixture.runtime.succeed(link)
                assertEquals(Ffi.STALE_SUBSCRIPTION, install(link))
                withTimeout(5_000) { fixture.scope.coroutineContext[Job]!!.children.toList().joinAll() }
                assertTrue(fixture.runtime.inputs.tryReceive().isFailure)
            }
        }
    }

    private fun install(link: TerminalLink): Int {
        val context = InstrumentationRegistry.getInstrumentation().context
        val bytes = context.assets.open("visual-checkpoint.snapshot").use { it.readBytes() }
        val checkpoint = ByteBuffer.allocateDirect(bytes.size).put(bytes).apply { flip() }
        return link.onCheckpoint(7, 2, 8, false, checkpoint)
    }

    private class Fixture : AutoCloseable {
        val runtime = InputRuntime()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        private val words = AndroidWords(InstrumentationRegistry.getInstrumentation().targetContext)
        private val commands = CommandSink(words).apply { transport = { Ffi.OK } }
        private val store = TerminalStore(commands).apply {
            receive(
                RuntimeEvent.parse(
                    """
                    {"type":"sessions.snapshot","accountUserId":null,"accountEpoch":0,"sessions":[{
                    "id":"fixture","incarnationId":"fixture-process","kind":"remote","name":"Fixture",
                    "isOwner":true,"status":"running","connectionState":"connected","working":false,
                    "attention":false,"hidden":false,"sharedWith":[],"connectedUsers":[],
                    "prompt":false,"repository":false,"hostName":"Fixture"}]}
                    """.trimIndent().encodeToByteArray()
                )
            )
        }
        val connection = TerminalConnection("fixture", runtime, commands, store, scope, words).apply { show(true) }

        override fun close() {
            connection.close()
            scope.cancel()
        }
    }

    private class InputRuntime : TerminalRuntime {
        val connections = Channel<TerminalLink>(Channel.UNLIMITED)
        val inputs = Channel<ByteArray>(Channel.UNLIMITED)

        @Volatile
        private var connected: TerminalLink? = null

        fun succeed(link: TerminalLink) {
            connected = link
            link.onConnectResult(Ffi.OK)
        }

        override fun connect(sessionId: String, link: TerminalLink): Int {
            connections.trySend(link)
            return Ffi.OK
        }

        override fun disconnect(sessionId: String, link: TerminalLink) {
            if (connected === link) connected = null
        }

        override fun input(sessionId: String, incarnationId: String, link: TerminalLink, bytes: ByteArray): Int {
            if (connected !== link) return Ffi.STALE_SUBSCRIPTION
            inputs.trySend(bytes.copyOf())
            return Ffi.OK
        }
    }
}
