package com.kodosi.mobile.sharing

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class SharingApprovalsTest {
    private val sent = SentCommands()
    private val approvals = SharingApprovals(sent.sink)

    @Test
    fun eachQuestionShowsOnceInTheOrderItCameUntilItEnds() {
        approvals.receive(asked("a"))
        approvals.receive(asked("b"))
        approvals.receive(asked("a"))
        assertEquals(listOf("a", "b"), shown())

        approvals.receive(event("sharing.approvalEnded", """"approvalId":"a","outcome":"expired""""))
        assertEquals(listOf("b"), shown())
    }

    @Test
    fun anAnswerReachesTheRuntimeAndTheNextQuestionShows() {
        approvals.receive(asked("a"))
        approvals.receive(asked("b"))

        approvals.answer(approvals.waiting.value.first(), allow = false)
        assertEquals(listOf("b"), shown())
        val answer = sent.last("sharing.answer")
        assertEquals("a", answer.string("approvalId"))
        assertEquals(JsonPrimitive(false), answer["allow"])
    }

    private fun asked(id: String) = event(
        "sharing.approval",
        """"approvalId":"$id","operation":"session.share","title":"Share Review with Ana?",
           "message":"Ana gets full control.","confirm":"Share","from":"app""""
    )

    private fun shown() = approvals.waiting.value.map { it.approvalId }
}
