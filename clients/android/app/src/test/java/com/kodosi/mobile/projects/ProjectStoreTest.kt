package com.kodosi.mobile.projects

import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProjectStoreTest {
    private val sent = SentCommands()
    private val store = ProjectStore(sent.sink).also { it.reset("ana") }

    @Test
    fun olderHistoryKeepsMessageOrderAndTheNewestTaskVersion() {
        snapshot(ProjectSnapshot("project", listOf(message(3), message(4)), listOf(task(4)), hasOlder = true))
        snapshot(ProjectSnapshot("project", listOf(message(1), message(2), message(3)), listOf(task(1))))
        val project = store.state.value.snapshots.getValue("project")
        assertEquals(listOf(1L, 2L, 3L, 4L), project.messages.map { it.sequence })
        assertEquals(4L, project.tasks.single().version)
        assertEquals(false, project.hasOlder)
        assertTrue(store.state.value.unread.isEmpty())
    }

    @Test
    fun marksOnlyUnseenMessagesWhileThePersonIsAway() {
        snapshot(ProjectSnapshot("project", listOf(message(1))))
        snapshot(ProjectSnapshot("project", listOf(message(2))))
        assertEquals(setOf("project"), store.state.value.unread)
        store.markRead("project")
        assertEquals("2", sent.last("project.seen")["sequence"].toString())
        store.reading = "project"
        snapshot(ProjectSnapshot("project", listOf(message(3))))
        assertTrue(store.state.value.unread.isEmpty())
        store.reading = null
        snapshot(ProjectSnapshot("project", listOf(message(4))))
        assertEquals(setOf("project"), store.state.value.unread)
        store.reset("bo")
        assertTrue(store.state.value.snapshots.isEmpty())
        assertTrue(store.state.value.unread.isEmpty())
    }

    @Test
    fun creationUsesItsRequestIdAsTheProjectId() = runTest {
        val creating = async(start = CoroutineStart.UNDISPATCHED) { store.create(" Work ") }
        val command = sent.last("project.create")
        sent.reply("project.create")
        assertEquals(command.string("requestId"), creating.await())
        assertEquals("Work", command.string("name"))
    }

    private fun snapshot(value: ProjectSnapshot) {
        store.receive(event("project.snapshot", """"content":${RuntimeJson.encodeToString(value)}"""))
    }

    private fun message(sequence: Long) =
        ProjectMessage("message-$sequence", sequence, "bo", "Bo", text = "Hello", createdAt = "2026-11-10T00:00:00Z")

    private fun task(version: Long) = ProjectTask("task", version, "Test", "")
}
