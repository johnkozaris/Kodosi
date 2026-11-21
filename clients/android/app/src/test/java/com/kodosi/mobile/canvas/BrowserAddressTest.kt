package com.kodosi.mobile.canvas

import androidx.compose.foundation.text.input.insert
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BrowserAddressTest {
    @Test
    fun theAddressFollowsTheActiveTabUntilThePersonTypes() {
        val address = BrowserAddress("https://kodosi.com/")

        address.follow("https://kodosi.com/docs")
        assertEquals("https://kodosi.com/docs", address.state.text.toString())
        assertFalse(address.edited)

        address.state.edit { insert(length, "/security") }
        address.follow("https://example.com/")
        assertTrue(address.edited)
        assertEquals("https://kodosi.com/docs/security", address.state.text.toString())

        address.place("https://kodosi.com/docs/security")
        address.follow("https://kodosi.com/docs/security#keys")
        assertFalse(address.edited)
        assertEquals("https://kodosi.com/docs/security#keys", address.state.text.toString())
    }

    @Test
    fun aNewTabKeepsTheAddressEmptyUntilItsPageOpens() {
        val address = BrowserAddress("https://kodosi.com/")

        address.place("")
        address.follow("about:blank")
        assertEquals("", address.state.text.toString())

        address.follow("https://kodosi.com/docs")
        assertEquals("https://kodosi.com/docs", address.state.text.toString())
    }
}
