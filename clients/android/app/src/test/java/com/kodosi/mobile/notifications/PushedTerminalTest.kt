package com.kodosi.mobile.notifications

import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.ProgramStatus
import org.junit.Assert.assertEquals
import org.junit.Test

class PushedTerminalTest {
    private val first = "0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d"
    private val second = "0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e"

    @Test
    fun theServersListNamesEachTerminalWithItsAgentStateAndSound() {
        assertEquals(
            listOf(
                PushedTerminal(
                    first,
                    AgentKind.CLAUDE,
                    ProgramStatus(ProgramStatus.State.BLOCKED, ProgramStatus.Kind.PERMISSION),
                    sound = true
                ),
                PushedTerminal(second, AgentKind.SHELL, ProgramStatus(ProgramStatus.State.DONE), sound = false)
            ),
            PushedTerminal.list("$first:claude:blocked:permission:1 $second::done::0")
        )
        assertEquals(
            ProgramStatus(ProgramStatus.State.ERROR),
            PushedTerminal.list("$first:codex:error::0").single().status
        )
    }
}
