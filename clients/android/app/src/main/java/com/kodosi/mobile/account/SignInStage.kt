package com.kodosi.mobile.account

import java.net.URI

sealed interface DeviceTrust {
    data object Choose : DeviceTrust

    data object Requesting : DeviceTrust

    data class PendingApproval(val code: String, val qr: List<String> = emptyList()) : DeviceTrust

    data object Joining : DeviceTrust

    data object Recovering : DeviceTrust

    data object Resetting : DeviceTrust

    data class Waiting(val readyAtMs: Long) : DeviceTrust

    data class Failed(val message: String) : DeviceTrust
}

sealed interface SignInStage {
    data object Idle : SignInStage

    data object Starting : SignInStage

    data object AwaitingBrowser : SignInStage

    data object Finalizing : SignInStage

    data class TrustingDevice(val trust: DeviceTrust) : SignInStage

    data object SignedIn : SignInStage

    data class Failed(val message: String) : SignInStage

    val isPending: Boolean
        get() = this is Starting || this is AwaitingBrowser || this is Finalizing || this is TrustingDevice
}

/** The system browser opens a sign-in page only over HTTPS, or over HTTP on this device. */
object ExternalAuthUrl {
    private val loopback = setOf("127.0.0.1", "localhost", "::1", "[::1]")

    fun parse(raw: String): String? {
        val uri = runCatching { URI(raw) }.getOrNull() ?: return null
        val scheme = uri.scheme?.lowercase()
        val host = uri.host
        val allowed = scheme == "https" || (scheme == "http" && host in loopback)
        return raw.takeIf { allowed && !host.isNullOrEmpty() && uri.userInfo == null }
    }
}
