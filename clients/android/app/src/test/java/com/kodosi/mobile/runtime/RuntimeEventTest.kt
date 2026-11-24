package com.kodosi.mobile.runtime

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
}
