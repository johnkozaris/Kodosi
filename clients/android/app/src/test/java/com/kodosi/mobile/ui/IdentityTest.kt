package com.kodosi.mobile.ui

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Test

class IdentityTest {
    @Test
    fun identitiesKeepTheirAppleColorsIncludingUtf8Identifiers() {
        assertEquals(0xE5F8229346420630uL, identityHash("friend-123"))
        assertEquals(Color(0xFF8573D1), personTint("friend-123"))
        assertEquals(Color(0xFFD9A441), roomTint("friend-123"))
        assertEquals(0x939A8B7EFC850567uL, identityHash("δοκιμή"))
        assertEquals(Color(0xFF2F92A3), personTint("δοκιμή"))
        assertEquals(0xD6C3EE23C573FE26uL, identityHash("友人"))
        assertEquals(Color(0xFFB8743C), roomTint("友人"))
    }
}
