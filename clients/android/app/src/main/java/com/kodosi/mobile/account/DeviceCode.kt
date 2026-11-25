package com.kodosi.mobile.account

// Prefix check only; full validation is done by the runtime.
object DeviceCode {
    fun join(text: String): String? = scanned(text, "join")

    fun approve(text: String): String? = scanned(text, "approve")

    private fun scanned(text: String, way: String): String? =
        text.trim().takeIf { it.startsWith("kodosi://device/$way/") }
}
