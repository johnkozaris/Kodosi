package com.kodosi.mobile.terminals

import org.junit.Assert.assertEquals
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
        attention = attention,
        workingDir = workingDir,
        status = RuntimeSession.Status.RUNNING,
        connectionState = RuntimeSession.Connection.CONNECTED
    )

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
            programStatus = ProgramStatus(state = ProgramStatus.State.BLOCKED, kind = kind)
        )

        assertEquals(StatusForm.QUESTION, waiting(ProgramStatus.Kind.QUESTION).sign())
        assertEquals(StatusForm.KEY, waiting(ProgramStatus.Kind.AUTH).sign())
        assertEquals(StatusForm.HAND, waiting(null).sign())
    }

    @Test
    fun signsAFinishedTerminalOnlyUntilItIsSeen() {
        val done = session("a", programStatus = ProgramStatus(state = ProgramStatus.State.DONE), attention = true)

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
