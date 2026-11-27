package com.kodosi.mobile.sharing

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ConfirmGateTest {
    @Test
    fun theConfirmOpensOnlyAfterTheTickAndAWholeReadTimeInFrontInOneStretch() {
        val gate = ConfirmGate(readMillis = 5_000)
        gate.front(0)
        gate.front(4_900)
        gate.away()
        gate.ticked = true
        gate.front(10_000)
        gate.front(14_900)
        assertFalse(gate.opens)

        gate.front(15_000)
        assertTrue(gate.opens)

        gate.away()
        gate.ticked = false
        gate.front(20_000)
        assertFalse(gate.opens)
        gate.ticked = true
        assertTrue(gate.opens)
    }
}
