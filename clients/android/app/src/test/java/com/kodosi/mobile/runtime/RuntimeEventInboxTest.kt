package com.kodosi.mobile.runtime

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class RuntimeEventInboxTest {
    @Test
    fun aFullInboxDropsAllThatWaitsAndSaysSoBeforeTheNewerEvents() = runTest {
        val small = json("a")
        val large = json("a".repeat(2 * small.size))
        val inbox = RuntimeEventInbox(maxBytes = 3L * small.size, maxCount = 2)
        inbox.push(small)
        inbox.push(small)
        inbox.push(small)
        inbox.push(json("b"))
        inbox.push(large)
        inbox.push(json("c"))
        inbox.push(json("d"))

        assertEquals(RuntimeEventInbox.Arrival.Behind(5, 4L * small.size + large.size), inbox.next())
        assertEquals("c", type(inbox.next()))
        assertEquals("d", type(inbox.next()))
    }

    @Test
    fun anEventThatCannotBeReadLeavesTheNextOneToCome() = runTest {
        val inbox = RuntimeEventInbox()
        inbox.push("[]".encodeToByteArray())
        inbox.push(json("a"))

        assertEquals(RuntimeEventInbox.Arrival.Unreadable("an event is not an object"), inbox.next())
        assertEquals("a", type(inbox.next()))
    }

    private fun json(type: String) = """{"type":"$type","accountUserId":null,"accountEpoch":0}""".encodeToByteArray()

    private fun type(arrival: RuntimeEventInbox.Arrival?) = (arrival as RuntimeEventInbox.Arrival.Event).event.type
}
