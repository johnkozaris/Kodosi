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
    fun ordersTheFoldersOfAComputerByPathWithTheTerminalsWithoutAFolderLast() {
        val folders = TerminalCatalog.folders(
            listOf(
                session("a", workingDir = "/home/maya/workspace"),
                session("b", workingDir = "/home/maya/project"),
                session("c"),
                session("d", workingDir = "/home/maya/project-amber-gecko"),
                session("e", workingDir = "/home/maya/project")
            )
        )

        assertEquals(
            listOf("/home/maya/project", "/home/maya/project-amber-gecko", "/home/maya/workspace", null),
            folders.map { it.first }
        )
        assertEquals(listOf("b", "e"), folders.first().second.map { it.id })
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
