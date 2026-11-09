package com.kodosi.mobile.runtime

import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CommandSinkTest {
    private val sent = SentCommands()
    private val sink = sent.sink

    @Test
    fun stampsEachCommandWithTheAccount() {
        sink.setAccount("ana", 4)
        sink.perform("devices.refresh")

        val command = sent.last("devices.refresh")
        assertEquals("ana", command.string("accountUserId"))
        assertEquals(JsonPrimitive(4), command["accountEpoch"])
        assertTrue(command.string("requestId").isNotEmpty())
    }

    @Test
    fun sendsNoAccountBeforeOneIsKnown() {
        sink.perform("session.list")

        assertEquals(JsonNull, sent.last("session.list")["accountUserId"])
    }

    @Test
    fun pairsAnAnswerWithItsRequest() = runTest {
        val pending = async(start = CoroutineStart.UNDISPATCHED) { sink.request("session.openRemote") }
        val requestId = sent.last("session.openRemote").string("requestId")

        assertEquals(CommandSink.Delivery.APP, sink.receive(answer("session.result", requestId, "session.list")))
        assertEquals(CommandSink.Delivery.APP, sink.receive(answer("devices.result", requestId, "session.openRemote")))
        val result = answer("session.result", requestId, "session.openRemote")
        assertEquals(CommandSink.Delivery.CALLER, sink.receive(result))
        assertEquals(result, pending.await())
    }

    @Test
    fun failsARequestWithTheMessageOfItsError() = runTest {
        val pending = async(start = CoroutineStart.UNDISPATCHED) { runCatching { sink.request("session.openRemote") } }
        val requestId = sent.last("session.openRemote").string("requestId")

        sink.receive(
            answer("session.error", requestId, "session.openRemote", """"message":"The computer is offline."""")
        )
        assertEquals("The computer is offline.", pending.await().exceptionOrNull()?.message)
    }

    @Test
    fun failsARequestThatTheRuntimeRefuses() = runTest {
        sent.result = Ffi.BUSY

        val failure = runCatching { sink.request("session.openRemote") }.exceptionOrNull()
        assertEquals(TestWords.rejected(Ffi.BUSY), failure?.message)
    }

    @Test
    fun anotherAccountEndsTheRequestsOfTheLastOne() = runTest {
        sink.setAccount("ana", 1)
        val pending = async(start = CoroutineStart.UNDISPATCHED) { runCatching { sink.request("session.openRemote") } }

        sink.setAccount("bo", 2)
        assertEquals(TestWords.accountChanged, pending.await().exceptionOrNull()?.message)
    }

    @Test
    fun reportsACommandThatCouldNotBeSent() {
        val failures = mutableListOf<String>()
        sink.onFailure = { failures += it }
        sink.transport = null

        sink.perform("devices.refresh")
        assertEquals(listOf(TestWords.unavailable), failures)
    }
}
