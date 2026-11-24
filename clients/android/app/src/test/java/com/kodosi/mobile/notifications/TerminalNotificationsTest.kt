package com.kodosi.mobile.notifications

import android.Manifest
import android.app.Application
import android.app.Notification
import android.app.NotificationManager
import com.kodosi.mobile.AppPreferences
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.TerminalNotifications
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
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
    private val review = session("0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c01", "Review", "work-key", "Work computer")
    private val build = session("0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c02", "Build", "work-key", "Work computer")

    @Before
    fun allowNotifications() {
        shadowOf(application).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
    }

    @Test
    fun aColdReceiverUsesOnlyItsAccountCacheForTheTextAndTheTap() {
        TerminalNotifications(application, preferences).observe(listOf(review), "ana")
        val receiver = TerminalNotifications(application, preferences)
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1"))

        val notification = terminal(review.id)!!
        assertEquals("Review", notification.extras.getString(Notification.EXTRA_TITLE))
        assertTrue(notification.extras.getString(Notification.EXTRA_TEXT)!!.contains("Work computer"))
        assertEquals(Notification.VISIBILITY_PRIVATE, notification.visibility)
        val target = shadowOf(notification.contentIntent).savedIntent
        assertEquals("ana", target.getStringExtra(TerminalNotifications.ACCOUNT))
        assertEquals(review.id, target.getStringExtra(TerminalNotifications.TERMINAL))
        receiver.showing(review.id)
        assertTrue(manager.activeNotifications.isEmpty())
    }

    @Test
    fun eachListPostsOnlyNewOrChangedTerminalsAndTakesBackTheOthers() {
        val receiver = signedIn()
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1 ${build.id}:codex:done::0"))
        val waiting = terminal(review.id)!!
        val done = terminal(build.id)!!

        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:0 ${build.id}:codex:error::0"))
        assertSame(waiting, terminal(review.id))
        assertNotSame(done, terminal(build.id))

        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:question:1"))
        assertNotSame(waiting, terminal(review.id))
        assertNull(terminal(build.id))

        receiver.receive(PushedTerminal.list(""))
        assertTrue(manager.activeNotifications.isEmpty())
    }

    @Test
    fun aTerminalThatIsSeenElsewhereOrGoneLosesItsNotification() {
        val receiver = signedIn(review.copy(attention = true), build.copy(attention = true))
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1 ${build.id}:claude:done::0"))

        receiver.observe(listOf(review, build.copy(attention = true)), "ana")
        assertNull(terminal(review.id))
        receiver.observe(listOf(review), "ana")
        assertTrue(manager.activeNotifications.isEmpty())
    }

    @Test
    fun disabledNotificationsStaySilentAndAppLockHidesTheirContent() {
        val receiver = signedIn()
        preferences.update { it.copy(notifications = false) }
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1"))
        assertTrue(manager.activeNotifications.isEmpty())

        preferences.update { it.copy(notifications = true, lock = true) }
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1"))
        assertEquals(Notification.VISIBILITY_SECRET, terminal(review.id)!!.visibility)
    }

    @Test
    fun changingAccountsCannotReuseThePreviousPersonsTerminalNames() {
        val receiver = signedIn()
        receiver.account("bob")
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1"))

        val notification = terminal(review.id)!!
        assertEquals(
            application.getString(R.string.terminal_notification_title),
            notification.extras.getString(Notification.EXTRA_TITLE)
        )
        assertFalse(notification.extras.getString(Notification.EXTRA_TEXT)!!.contains("Work computer"))
        receiver.account(null)
        receiver.receive(PushedTerminal.list("${review.id}:claude:blocked:permission:1"))
        assertTrue(manager.activeNotifications.isEmpty())
    }

    private val Notification.isSummary: Boolean get() = flags and Notification.FLAG_GROUP_SUMMARY != 0

    private fun signedIn(vararg sessions: RuntimeSession) = TerminalNotifications(application, preferences).also {
        it.observe(sessions.toList().ifEmpty { listOf(review, build) }, "ana")
    }

    private fun terminal(id: String): Notification? =
        manager.activeNotifications.firstOrNull { it.tag == id && !it.notification.isSummary }?.notification

    private fun session(id: String, name: String, computer: String, computerName: String) = RuntimeSession(
        id = id,
        incarnationId = "019363fc-6fca-7000-8000-000000000002",
        name = name,
        kind = RuntimeSession.Kind.REMOTE,
        status = RuntimeSession.Status.RUNNING,
        connectionState = RuntimeSession.Connection.CONNECTED,
        isWorking = false,
        isOwner = true,
        hostDeviceId = computer,
        hostName = computerName
    )
}
