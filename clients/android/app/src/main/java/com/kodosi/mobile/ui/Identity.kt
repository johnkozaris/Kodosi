package com.kodosi.mobile.ui

import androidx.compose.ui.graphics.Color

internal fun identityHash(text: String): ULong {
    var hash = 0xCBF29CE484222325uL
    for (byte in text.encodeToByteArray()) {
        hash = (hash xor byte.toUByte().toULong()) * 0x100000001B3uL
    }
    return hash
}

private val personColors = listOf(
    Color(0xFF3C8DB5),
    Color(0xFF8573D1),
    Color(0xFF2F92A3),
    Color(0xFFC2659A),
    Color(0xFFB58428),
    Color(0xFFB362B6),
    Color(0xFF607FCC)
)
private val roomColors = listOf(
    Color(0xFFD98A5F),
    Color(0xFFD9A441),
    Color(0xFFC96F7E),
    Color(0xFF8C7BD1),
    Color(0xFF4F9BB0),
    Color(0xFFB8743C),
    Color(0xFFA86AAE)
)

internal fun personTint(id: String): Color = personColors[(identityHash(id) % personColors.size.toULong()).toInt()]
internal fun roomTint(id: String): Color = roomColors[(identityHash(id) % roomColors.size.toULong()).toInt()]
