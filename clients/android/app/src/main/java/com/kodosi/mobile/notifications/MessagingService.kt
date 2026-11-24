package com.kodosi.mobile.notifications

import android.annotation.SuppressLint
import android.os.Handler
import android.os.Looper
import android.util.Log
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import com.kodosi.mobile.KodosiApplication
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull

// Android lint knows only the deprecated onNewToken, not onRegistered that replaces it.
@SuppressLint("MissingFirebaseInstanceTokenRefresh")
class MessagingService : FirebaseMessagingService() {
    override fun onMessageReceived(message: RemoteMessage) {
        val pushed = message.data["pushed"] ?: return
        (application as KodosiApplication).notifications.receive(PushedTerminal.list(pushed))
    }

    override fun onRegistered(installationId: String) {
        if (installationId.isBlank()) return
        if (waiting.getAndSet(null)?.complete(installationId) == true) return
        synchronize(force = true, installationId)
    }

    override fun onDeletedMessages() {
        synchronize(force = false)
    }

    private fun synchronize(force: Boolean, installationId: String? = null) {
        val app = application as KodosiApplication
        if (!app.preferences.state.value.notifications || !app.notifications.hasAccount) return
        try {
            PushWorker.enqueue(app)
        } catch (failure: Exception) {
            Log.w("Kodosi", "Could not schedule push registration.", failure)
        }
        Handler(Looper.getMainLooper()).post {
            app.model.updatePush(force, installationId)
        }
    }

    companion object {
        private const val REGISTRATION_TIMEOUT_MS = 15_000L
        private val waiting = AtomicReference<CompletableDeferred<String>?>(null)

        internal suspend fun awaitRegistration(trigger: suspend () -> Unit): String? {
            val pending = CompletableDeferred<String>()
            waiting.getAndSet(pending)?.cancel()
            return try {
                trigger()
                withTimeoutOrNull(REGISTRATION_TIMEOUT_MS) { pending.await() }
            } finally {
                waiting.compareAndSet(pending, null)
            }
        }
    }
}
