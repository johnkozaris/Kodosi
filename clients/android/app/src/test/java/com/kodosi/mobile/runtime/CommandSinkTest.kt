package com.kodosi.mobile.runtime

import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CommandSinkTest {
    private val sent = SentCommands()
    private val sink = sent.sink

    @Test
    fun aRequestWhoseAnswerTheAppDroppedCannotTellAndItsLateAnswerReachesNobody() = runTest {
        val pending = async(start = CoroutineStart.UNDISPATCHED) { runCatching { sink.request("project.create") } }
        val requestId = sent.last("project.create").string("requestId")

        sink.abandonPending()
        val failure = pending.await().exceptionOrNull()
        assertTrue(failure is LostAnswer)
        assertEquals(TestWords.outcomeUnknown, failure?.message)
        val late = answer("project.error", requestId, "project.create", """"message":"The name is taken."""")
        assertEquals(CommandSink.Delivery.CALLER, sink.receive(late))
    }

    @Test
    fun anotherAccountEndsTheRequestsOfTheLastOne() = runTest {
        sink.setAccount("ana", 1)
        val pending = async(start = CoroutineStart.UNDISPATCHED) { runCatching { sink.request("session.openRemote") } }

        sink.setAccount("bo", 2)
        assertEquals(TestWords.accountChanged, pending.await().exceptionOrNull()?.message)
    }

    @Test
    fun aRequestThatMayWaitForTheOwnersOkOutlastsTheUsualMinute() = runTest {
        val pending = async(start = CoroutineStart.UNDISPATCHED) { runCatching { sink.request("session.share") } }

        delay(2 * 60_000L)
        assertTrue(pending.isActive)
        sent.reply("session.share")
        assertTrue(pending.await().isSuccess)
    }

    @Test
    fun aRequestTheOwnerRefusedEndsAsRefusedSoNoOneReportsIt() = runTest {
        val pending = async(start = CoroutineStart.UNDISPATCHED) { runCatching { sink.request("session.share") } }
        val requestId = sent.last("session.share").string("requestId")

        sink.receive(
            answer(
                "session.error",
                requestId,
                "session.share",
                """"message":"You did not allow this.","refused":true"""
            )
        )
        assertTrue(pending.await().exceptionOrNull() is Refused)
    }
}
