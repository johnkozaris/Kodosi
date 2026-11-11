package com.kodosi.mobile.notifications

import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.ProgramStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TerminalAlertTest {
    private val payload = mapOf(
        "terminal" to "019363fc-6fca-7000-8000-000000000001",
        "state" to "blocked",
        "kind" to "question",
        "agent" to "claude"
    )

    @Test
    fun stateAndAgentMetadataAreNotUserIdentityOrTerminalText() {
        val alert = TerminalAlert.parse(payload)!!
        assertEquals(AgentKind.CLAUDE, alert.agent)
        assertEquals(ProgramStatus(ProgramStatus.State.BLOCKED, ProgramStatus.Kind.QUESTION), alert.status)
        assertNull(alert.status.caption)
        assertEquals(ProgramStatus.Kind.AUTH, TerminalAlert.parse(payload + ("kind" to "auth"))!!.status.kind)
        assertEquals(AgentKind.SHELL, TerminalAlert.parse(payload - "agent")!!.agent)
        assertEquals(
            ProgramStatus(ProgramStatus.State.DONE),
            TerminalAlert.parse(mapOf("terminal" to payload.getValue("terminal"), "state" to "done"))!!.status
        )
    }

    @Test
    fun aMalformedOrNonAlertMessageDoesNotMakeANotification() {
        assertNull(TerminalAlert.parse(payload - "terminal"))
        assertNull(TerminalAlert.parse(payload + ("terminal" to "not-a-terminal")))
        assertNull(TerminalAlert.parse(payload + ("state" to "working")))
    }
}
