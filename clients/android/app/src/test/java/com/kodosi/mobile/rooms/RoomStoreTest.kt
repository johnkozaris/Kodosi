package com.kodosi.mobile.rooms

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

class RoomStoreTest {
    private val sent = SentCommands()
    private val store = RoomStore(sent.sink).also { it.reset("ana") }

    @Test
    fun olderHistoryKeepsMessageOrderAndTheNewestTaskVersion() {
        snapshot(RoomSnapshot("room", listOf(message(3), message(4)), listOf(task(4)), hasOlder = true))
        snapshot(RoomSnapshot("room", listOf(message(1), message(2), message(3)), listOf(task(1))))
        val room = store.state.value.snapshots.getValue("room")
        assertEquals(listOf(1L, 2L, 3L, 4L), room.messages.map { it.sequence })
        assertEquals(4L, room.tasks.single().version)
        assertEquals(false, room.hasOlder)
        assertTrue(store.state.value.unread.isEmpty())
    }

    @Test
    fun marksOnlyUnseenMessagesWhileThePersonIsAway() {
        snapshot(RoomSnapshot("room", listOf(message(1))))
        snapshot(RoomSnapshot("room", listOf(message(2))))
        assertEquals(setOf("room"), store.state.value.unread)
        store.markRead("room")
        assertEquals("2", sent.last("mission.seen")["sequence"].toString())
        store.reading = "room"
        snapshot(RoomSnapshot("room", listOf(message(3))))
        assertTrue(store.state.value.unread.isEmpty())
        store.reading = null
        snapshot(RoomSnapshot("room", listOf(message(4))))
        assertEquals(setOf("room"), store.state.value.unread)
        store.reset("bo")
        assertTrue(store.state.value.snapshots.isEmpty())
        assertTrue(store.state.value.unread.isEmpty())
    }

    @Test
    fun creationUsesItsRequestIdAsTheRoomId() = runTest {
        val creating = async(start = CoroutineStart.UNDISPATCHED) { store.create(" Work ") }
        val command = sent.last("mission.create")
        sent.reply("mission.create")
        assertEquals(command.string("requestId"), creating.await())
        assertEquals("Work", command.string("name"))
    }

    private fun snapshot(value: RoomSnapshot) {
        store.receive(event("room.snapshot", """"room":${RuntimeJson.encodeToString(value)}"""))
    }

    private fun message(sequence: Long) =
        RoomMessage("message-$sequence", sequence, "bo", "Bo", text = "Hello", createdAt = "2026-11-10T00:00:00Z")

    private fun task(version: Long) = RoomTask("task", version, "Test", "")
}
