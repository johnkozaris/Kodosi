package com.kodosi.mobile.account

import android.app.Activity
import android.app.KeyguardManager
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.CancellationSignal
import android.os.SystemClock
import com.kodosi.mobile.AppPreferences
import com.kodosi.mobile.R
import kotlin.coroutines.resume
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.suspendCancellableCoroutine

class AppLock(private val preferences: AppPreferences) {
    private val mutableLocked = MutableStateFlow(preferences.state.value.lock)
    val locked: StateFlow<Boolean> = mutableLocked
    private var left: Long? = null
    private var asking = false
    private var asked = false

    fun background() {
        if (asking) return
        if (left == null) left = SystemClock.elapsedRealtime()
        asked = false
    }

    fun foreground() {
        val now = SystemClock.elapsedRealtime()
        if (preferences.state.value.lock && left?.let { now - it >= AWAY_MS } == true) mutableLocked.value = true
        left = null
    }

    suspend fun unlock(activity: Activity, automatic: Boolean = false) {
        if (!locked.value || asking || (automatic && asked)) return
        asking = true
        asked = true
        try {
            if (!canLock(activity)) {
                preferences.update { it.copy(lock = false) }
                mutableLocked.value = false
            } else if (prove(activity)) {
                mutableLocked.value = false
            }
        } finally {
            asking = false
        }
    }

    suspend fun enable(activity: Activity, on: Boolean) {
        if (on && (!canLock(activity) || !prove(activity))) return
        preferences.update { it.copy(lock = on) }
        mutableLocked.value = false
    }

    companion object {
        const val AWAY_MS = 60_000L

        fun canLock(activity: Activity): Boolean = activity.getSystemService(KeyguardManager::class.java).isDeviceSecure

        private suspend fun prove(activity: Activity): Boolean = suspendCancellableCoroutine { continuation ->
            val cancellation = CancellationSignal()
            val builder = BiometricPrompt.Builder(activity)
                .setTitle(activity.getString(R.string.unlock))
                .setDescription(activity.getString(R.string.unlock_reason))
                .setAllowedAuthenticators(
                    BiometricManager.Authenticators.BIOMETRIC_STRONG or
                        BiometricManager.Authenticators.DEVICE_CREDENTIAL
                )
            continuation.invokeOnCancellation { cancellation.cancel() }
            builder.build().authenticate(
                cancellation,
                activity.mainExecutor,
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                        if (continuation.isActive) continuation.resume(true)
                    }

                    override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                        if (continuation.isActive) continuation.resume(false)
                    }
                }
            )
        }
    }
}
