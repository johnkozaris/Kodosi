package com.kodosi.mobile.notifications

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PushRegistrationTest {
    private val sent = SentCommands().apply { sink.setAccount("ana", 1) }

    @Test
    fun anApprovedDeviceRegistersOnceAndTokenRotationRegistersAgain() = runTest {
        var token = "first-token"
        val registration = PushRegistration(sent.sink, this, { token }, {})
        val initial = registration.update("ana", 1, true)
        runCurrent()
        registration.update("ana", 1, true)
        assertEquals(listOf("push.register"), sent.types)
        assertEquals("fcm", sent.last("push.register").string("platform"))
        assertFalse(sent.last("push.register").containsKey("environment"))
        sent.reply("push.register")
        runCurrent()
        assertTrue(registration.await(initial))

        registration.update("ana", 1, true)
        runCurrent()
        assertEquals(1, sent.commands.size)
        token = "rotated-token"
        val rotated = registration.update("ana", 1, true, force = true)
        runCurrent()
        assertEquals("rotated-token", sent.last("push.register").string("token"))
        sent.reply("push.register")
        runCurrent()
        assertTrue(registration.await(rotated))
    }

    @Test
    fun aTokenThatArrivesAfterSignOutCannotRegisterForTheNextAccount() = runTest {
        val token = CompletableDeferred<String>()
        var forgotten = false
        val registration = PushRegistration(sent.sink, this, { token.await() }, { forgotten = true })
        registration.update("ana", 1, true)
        runCurrent()
        sent.sink.setAccount(null, 2)
        val signedOut = registration.update(null, 2, false)
        token.complete("old-token")
        runCurrent()

        assertTrue(sent.commands.isEmpty())
        assertTrue(forgotten)
        assertTrue(registration.await(signedOut))
    }

    @Test
    fun turningNotificationsOffWaitsForAnInFlightRegistrationBeforeRemovingIt() = runTest {
        var forgotten = false
        val registration = PushRegistration(sent.sink, this, { "token" }, { forgotten = true })
        registration.update("ana", 1, true)
        runCurrent()
        val disabled = registration.update("ana", 1, false)
        runCurrent()
        assertEquals(listOf("push.register"), sent.types)

        sent.reply("push.register")
        runCurrent()
        assertEquals(listOf("push.register", "push.unregister"), sent.types)
        sent.reply("push.unregister")
        runCurrent()
        assertTrue(forgotten)
        assertTrue(registration.await(disabled))
    }

    @Test
    fun aFailedRegistrationCanRetryWithoutChangingTheAccount() = runTest {
        var online = false
        val registration = PushRegistration(
            sent.sink,
            this,
            { if (!online) error("No connection.") else "token" },
            {}
        )
        val failed = registration.update("ana", 1, true)
        runCurrent()
        assertFalse(registration.await(failed))
        assertEquals("No connection.", registration.issue.value)

        online = true
        val retry = registration.update("ana", 1, true, force = true)
        runCurrent()
        sent.reply("push.register")
        runCurrent()
        assertTrue(registration.await(retry))
        assertNull(registration.issue.value)
    }
}
