package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.RuntimeJson
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class TerminalCatalogTest {
    private fun session(
        id: String,
        isOwner: Boolean = true,
        hostDeviceId: String? = null,
        hostName: String? = null,
        ownerUserId: String? = null,
        ownerName: String? = null,
        programStatus: ProgramStatus? = null,
        waitState: ProgramStatus.State? = null,
        attention: Boolean = false,
        workingDir: String? = null
    ) = RuntimeSession(
        id = id,
        incarnationId = "i-$id",
        kind = RuntimeSession.Kind.REMOTE,
        name = id,
        isOwner = isOwner,
        hostDeviceId = hostDeviceId,
        hostName = hostName,
        ownerUserId = ownerUserId,
        ownerName = ownerName,
        programStatus = programStatus,
        waitState = waitState,
        attention = attention,
        workingDir = workingDir,
        status = RuntimeSession.Status.RUNNING,
        connectionState = RuntimeSession.Connection.CONNECTED,
        isWorking = false
    )

    @Test
    fun keepsTheNameAsTitleWhileTheSecondLineSaysAState() {
        val agent = session("Dusk Viper").copy(program = "claude", activity = "Fix the sign-in")

        assertEquals("Fix the sign-in" to "Dusk Viper", agent.cardLines(online = true, summaryFirst = true))
        assertEquals(
            "Dusk Viper" to "Fix the sign-in",
            agent.copy(status = RuntimeSession.Status.OFFLINE).cardLines(online = true, summaryFirst = true)
        )
        assertEquals("Dusk Viper" to "Fix the sign-in", agent.cardLines(online = false, summaryFirst = true))
    }

    @Test
    fun keepsAgentIdentityInHumanReadableMessageAndBrowserNames() {
        assertEquals(AgentKind.COPILOT, AgentKind.named("Copilot CLI"))
        assertEquals(AgentKind.CLAUDE, AgentKind.named("Claude Code"))
        assertEquals(AgentKind.ANTIGRAVITY, AgentKind.named("Antigravity CLI"))
        assertEquals(AgentKind.ANTIGRAVITY, AgentKind.of("agy"))
        assertEquals(AgentKind.SHELL, AgentKind.named("Build helper"))
        assertEquals(AgentKind.SHELL, AgentKind.named(null))
    }

    @Test
    fun usesTheRuntimeActivityAndWaitingStateInsteadOfInferringThemFromTheProgram() {
        val terminal = RuntimeJson.decodeFromString<RuntimeSession>(
            """{"id":"a","incarnationId":"i","kind":"remote","name":"Build",
                "status":"running","connectionState":"connected","working":false,
                "waiting":"done","activity":"Build complete","attention":true,
                "title":"\u280b Old title","programStatus":{"state":"working"}}"""
        )

        assertFalse(terminal.isWorking)
        assertEquals(ProgramStatus.State.DONE, terminal.waitState)
        assertEquals("Build complete", terminal.activity)
        assertEquals(StatusForm.DONE, terminal.sign())
    }

    @Test
    fun groupsYourTerminalsByComputerBeforeYourFriends() {
        val groups = TerminalCatalog.groups(
            listOf(
                session("shell", hostDeviceId = "mac", hostName = "John's MacBook Pro"),
                session("review", isOwner = false, ownerUserId = "bo", ownerName = "Bo"),
                session("build", hostDeviceId = "linux", hostName = "Atelier"),
                session("agent", hostDeviceId = "mac", hostName = "John's MacBook Pro"),
                session("stray")
            ),
            unknownComputer = "Another computer",
            sharedWithYou = "Shared with you"
        )

        assertEquals(
            listOf("Another computer", "Atelier", "John's MacBook Pro", "Bo"),
            groups.map { it.place.name }
        )
        assertEquals(listOf("shell", "agent"), groups[2].sessions.map { it.id })
        assertEquals(TerminalGroup.Place.Friend("bo", "Bo"), groups[3].place)
    }

    @Test
    fun signsWhatATerminalWaitsFor() {
        fun waiting(kind: ProgramStatus.Kind?) = session(
            "a",
            programStatus = ProgramStatus(state = ProgramStatus.State.BLOCKED, kind = kind),
            waitState = ProgramStatus.State.BLOCKED
        )

        assertEquals(StatusForm.QUESTION, waiting(ProgramStatus.Kind.QUESTION).sign())
        assertEquals(StatusForm.KEY, waiting(ProgramStatus.Kind.AUTH).sign())
        assertEquals(StatusForm.HAND, waiting(null).sign())
    }

    @Test
    fun signsAFinishedTerminalOnlyUntilItIsSeen() {
        val done = session("a", waitState = ProgramStatus.State.DONE, attention = true)

        assertEquals(StatusForm.DONE, done.sign())
        assertNull(done.sign(unseen = false))
        assertEquals(StatusForm.CHANGED, session("b", attention = true).sign())
    }

    @Test
    fun findsTerminalsByTheirComputerOrFolder() {
        val sessions = listOf(
            session("a", hostName = "Atelier"),
            session("b", workingDir = "/Users/john/Repos/Kodosi/")
        )

        assertEquals(listOf("a"), TerminalCatalog.matching(sessions, "atel").map { it.id })
        assertEquals(listOf("b"), TerminalCatalog.matching(sessions, " kodosi ").map { it.id })
        assertEquals(sessions, TerminalCatalog.matching(sessions, "  "))
    }
}
