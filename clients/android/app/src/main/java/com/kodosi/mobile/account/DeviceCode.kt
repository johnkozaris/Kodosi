package com.kodosi.mobile.account

/** A scanner or an opened link passes on only a text in the form that the runtime makes for this
 *  way; the runtime decides the rest. */
object DeviceCode {
    fun join(text: String): String? = scanned(text, "join")

    fun approve(text: String): String? = scanned(text, "approve")

    private fun scanned(text: String, way: String): String? =
        text.trim().takeIf { it.startsWith("kodosi://device/$way/") }
}
