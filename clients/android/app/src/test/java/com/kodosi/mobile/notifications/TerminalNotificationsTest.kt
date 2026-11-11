package com.kodosi.mobile.notifications

import android.Manifest
import android.app.Application
import android.app.Notification
import android.app.NotificationManager
import com.kodosi.mobile.AppPreferences
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.ProgramStatus
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.TerminalNotifications
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class TerminalNotificationsTest {
    private val application: Application = RuntimeEnvironment.getApplication()
    private val preferences = AppPreferences(application)
    private val manager = application.getSystemService(NotificationManager::class.java)
    private val session = RuntimeSession(
        id = "019363fc-6fca-7000-8000-000000000001",
        incarnationId = "019363fc-6fca-7000-8000-000000000002",
        name = "Review",
        kind = RuntimeSession.Kind.REMOTE,
        status = RuntimeSession.Status.RUNNING,
        connectionState = RuntimeSession.Connection.CONNECTED,
        isWorking = false,
        isOwner = true,
        hostDeviceId = "computer-key",
        hostName = "Work computer"
    )
    private val alert = TerminalAlert.parse(
        mapOf("terminal" to session.id, "state" to "blocked", "agent" to "claude")
    )!!

    @Before
    fun allowNotifications() {
        shadowOf(application).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
    }

    @Test
    fun aColdReceiverUsesOnlyItsAccountCacheAndAnUrgentWaitingChannel() {
        TerminalNotifications(application, preferences).observe(listOf(session), "ana")
        val receiver = TerminalNotifications(application, preferences)
        receiver.receive(alert)

        val notification = manager.activeNotifications.single().notification
        assertEquals("Review", notification.extras.getString(Notification.EXTRA_TITLE))
        assertTrue(notification.extras.getString(Notification.EXTRA_TEXT)!!.contains("Work computer"))
        assertEquals(
            NotificationManager.IMPORTANCE_HIGH,
            manager.getNotificationChannel(notification.channelId).importance
        )
        val target = shadowOf(notification.contentIntent).savedIntent
        assertEquals("ana", target.getStringExtra(TerminalNotifications.ACCOUNT))
        assertEquals(session.id, target.getStringExtra(TerminalNotifications.TERMINAL))
        receiver.showing(session.id)
        assertTrue(manager.activeNotifications.isEmpty())
    }

    @Test
    fun runtimeWaitingStartsAndClearsAnAlertWithoutAProgramStatus() {
        val receiver = TerminalNotifications(application, preferences)
        receiver.observe(listOf(session), "ana")
        receiver.observe(
            listOf(session.copy(waitState = ProgramStatus.State.DONE, activity = "Build complete")),
            "ana"
        )
        val notification = manager.activeNotifications.single().notification
        assertEquals("Review", notification.extras.getString(Notification.EXTRA_TITLE))
        assertTrue(notification.extras.getString(Notification.EXTRA_TEXT)!!.contains("Build complete"))

        receiver.observe(listOf(session.copy(isWorking = true)), "ana")
        assertTrue(manager.activeNotifications.isEmpty())
    }

    @Test
    fun foregroundAndDisabledNotificationsStaySilentAndAppLockHidesTheirContent() {
        val receiver = TerminalNotifications(application, preferences)
        receiver.observe(listOf(session), "ana")
        receiver.foreground = true
        receiver.receive(alert)
        assertTrue(manager.activeNotifications.isEmpty())

        receiver.foreground = false
        preferences.update { it.copy(notifications = false) }
        receiver.receive(alert)
        assertTrue(manager.activeNotifications.isEmpty())
        preferences.update { it.copy(notifications = true, lock = true) }
        receiver.receive(alert)
        assertEquals(Notification.VISIBILITY_SECRET, manager.activeNotifications.single().notification.visibility)
    }

    @Test
    fun anAlertForAHiddenTerminalStaysSilent() {
        val receiver = TerminalNotifications(application, preferences)
        receiver.observe(listOf(session.copy(hidden = true)), "ana")
        receiver.receive(alert)

        assertTrue(manager.activeNotifications.isEmpty())
    }

    @Test
    fun changingAccountsCannotReuseThePreviousPersonsTerminalNames() {
        val receiver = TerminalNotifications(application, preferences)
        receiver.observe(listOf(session), "ana")
        receiver.account("bob")
        receiver.receive(alert)

        val notification = manager.activeNotifications.single().notification
        assertEquals(
            application.getString(R.string.terminal_notification_title),
            notification.extras.getString(Notification.EXTRA_TITLE)
        )
        assertTrue(!notification.extras.getString(Notification.EXTRA_TEXT)!!.contains("Work computer"))
        receiver.account(null)
        receiver.receive(alert)
        assertTrue(manager.activeNotifications.isEmpty())
    }
}
