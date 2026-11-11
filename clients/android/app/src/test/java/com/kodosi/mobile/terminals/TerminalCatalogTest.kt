package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.RuntimeJson
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
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
    fun keepsAgentIdentityInHumanReadableMessageAndBrowserNames() {
        assertEquals(AgentKind.COPILOT, AgentKind.named("Copilot CLI"))
        assertEquals(AgentKind.CLAUDE, AgentKind.named("Claude Code"))
        assertEquals(AgentKind.GEMINI, AgentKind.named("Gemini CLI"))
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
    fun showsWorkWhenTheRuntimeReportsItWithoutAProgramStatusOrTitle() {
        val terminal = RuntimeJson.decodeFromString<RuntimeSession>(
            """{"id":"a","incarnationId":"i","kind":"remote","name":"Build",
                "status":"running","connectionState":"connected","working":true}"""
        )

        assertTrue(terminal.isWorking)
        assertNull(terminal.waitState)
        assertNull(terminal.activity)
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
