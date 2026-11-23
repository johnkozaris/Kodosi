package com.kodosi.mobile.runtime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class RuntimeEventTest {
    @Test
    fun rejectsEventsWithoutTypeOrAccount() {
        val invalid = listOf(
            """[]""",
            """{"accountUserId":null,"accountEpoch":0}""",
            """{"type":"","accountUserId":null,"accountEpoch":0}""",
            """{"type":7,"accountUserId":null,"accountEpoch":0}""",
            """{"type":"system.ready","accountEpoch":0}""",
            """{"type":"system.ready","accountUserId":5,"accountEpoch":0}""",
            """{"type":"system.ready","accountUserId":null}""",
            """{"type":"system.ready","accountUserId":null,"accountEpoch":"0"}"""
        )
        for (json in invalid) assertThrows(json, InvalidEvent::class.java) { event(json) }
    }

    @Test
    fun makesVersionSevenIdentifiersInTimeOrder() {
        val first = UuidV7.generate(now = 1_000)
        val second = UuidV7.generate(now = 2_000)

        assertEquals('7', first[14])
        assertEquals(true, first[19] in "89ab")
        assertEquals(true, first < second)
    }
}
