package com.kodosi.mobile.notifications

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.kodosi.mobile.AppPreferences
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.TerminalNotifications
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TerminalNotificationTest {
    @Test
    fun aColdReceiverPostsAndClearsAPrivateWaitingNotification() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val manager = context.getSystemService(NotificationManager::class.java)
        val preferences = AppPreferences(context)
        val previous = preferences.state.value
        val session = RuntimeSession(
            id = "019363fc-6fca-7000-8000-000000000001",
            incarnationId = "019363fc-6fca-7000-8000-000000000002",
            name = "Release validation",
            kind = RuntimeSession.Kind.REMOTE,
            status = RuntimeSession.Status.RUNNING,
            connectionState = RuntimeSession.Connection.CONNECTED,
            isWorking = false,
            isOwner = true,
            hostName = "Disposable computer"
        )
        val receiver = TerminalNotifications(context, preferences)
        instrumentation.uiAutomation.adoptShellPermissionIdentity(Manifest.permission.POST_NOTIFICATIONS)
        try {
            preferences.update { it.copy(notifications = true, lock = true) }
            receiver.observe(listOf(session), "native-notification-fixture")
            val cold = TerminalNotifications(context, preferences)
            cold.receive(
                TerminalAlert.parse(
                    mapOf("terminal" to session.id, "state" to "blocked", "agent" to "claude")
                )!!
            )
            val deadline = System.nanoTime() + 5_000_000_000L
            while (manager.activeNotifications.isEmpty() && System.nanoTime() < deadline) Thread.sleep(25)
            val notification = manager.activeNotifications.single { it.tag == session.id }.notification
            assertEquals(session.name, notification.extras.getString(Notification.EXTRA_TITLE))
            assertTrue(notification.extras.getString(Notification.EXTRA_TEXT)!!.contains(session.hostName!!))
            assertEquals(Notification.VISIBILITY_SECRET, notification.visibility)
            assertEquals(
                NotificationManager.IMPORTANCE_HIGH,
                manager.getNotificationChannel(notification.channelId).importance
            )
            assertTrue(notification.contentIntent.isImmutable)
            assertEquals(context.packageName, notification.contentIntent.creatorPackage)
            cold.showing(session.id)
            while (manager.activeNotifications.any { it.tag == session.id } && System.nanoTime() < deadline) {
                Thread.sleep(25)
            }
            assertTrue(manager.activeNotifications.none { it.tag == session.id })
        } finally {
            receiver.reset()
            preferences.update { previous }
            instrumentation.uiAutomation.dropShellPermissionIdentity()
        }
    }
}
