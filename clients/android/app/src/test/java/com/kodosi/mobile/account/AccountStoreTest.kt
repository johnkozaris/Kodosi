package com.kodosi.mobile.account

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.answer
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccountStoreTest {
    private val sent = SentCommands()
    private val notices = mutableListOf<String>()
    private val store = AccountStore(sent.sink, TestWords) {
        "el"
    }.also { it.notify = { message -> notices += message } }

    private val stage get() = store.state.value.stage

    @Test
    fun signsInThroughTheAppsBrowserInItsLanguage() {
        store.signIn()

        val start = sent.last("auth.login.start")
        assertEquals("app", start.string("browser"))
        assertEquals("el", start.string("language"))
        assertEquals(SignInStage.Starting, stage)
    }

    @Test
    fun opensOnlyPagesThatAreSafeToOpen() {
        store.signIn()
        store.receiveAuth(event("auth.browser", """"requestId":"r","url":"http://192.168.1.4/login""""))
        assertEquals(SignInStage.AwaitingBrowser(null), stage)

        store.receiveAuth(event("auth.browser", """"requestId":"r","url":"https://id.kodosi.com/login""""))
        assertEquals("https://id.kodosi.com/login", store.state.value.signInPage)
        store.openedSignInPage()
        assertNull(store.state.value.signInPage)
    }

    @Test
    fun passesTheBrowsersAnswerOnOnlyWhileItWaits() {
        store.finishSignIn("com.kodosi:/auth?code=1")
        assertFalse("auth.login.finish" in sent.types)

        store.signIn()
        store.receiveAuth(event("auth.browser", """"requestId":"r","url":"https://id.kodosi.com/login""""))
        store.finishSignIn("com.kodosi:/auth?code=1")
        assertEquals("com.kodosi:/auth?code=1", sent.last("auth.login.finish").string("url"))
        assertEquals(SignInStage.Finalizing, stage)
        store.finishSignIn("com.kodosi:/auth?code=1")
        assertEquals(1, sent.types.count { it == "auth.login.finish" })
    }

    @Test
    fun asksToTrustANewDeviceAfterSignIn() {
        store.reset("ana")
        store.receiveAuth(event("auth.ready", """"userId":"ana","displayName":"Ana","enrolled":false"""))

        assertEquals(SignInStage.TrustingDevice(DeviceTrust.Choose), stage)
        assertTrue(store.state.value.needsOnboarding)
    }

    @Test
    fun showsTheApprovalCodeThenFinishesWhenTheDeviceIsApproved() {
        store.reset("ana")
        store.receiveAuth(event("auth.ready", """"userId":"ana","enrolled":false"""))
        store.requestDeviceApproval()
        val requestId = sent.last("devices.link.startSelf").string("requestId")

        store.receiveDevices(answer("devices.result", requestId, "devices.link.startSelf", """"code":"K7Q-42M""""))
        assertEquals(SignInStage.TrustingDevice(DeviceTrust.PendingApproval("K7Q-42M")), stage)

        store.receiveDevices(event("devices.link.selfResolved", """"outcome":"approved""""))
        assertEquals("devices.refresh", sent.types.last())
        store.receiveDevices(
            event(
                "devices.list",
                """"selfDeviceId":"phone","localDeviceEnrolled":true,"devices":""" +
                    """[{"deviceId":"phone","label":"Pixel","certIssuedAtMs":1,"recoveryKey":false}]"""
            )
        )
        assertEquals(SignInStage.SignedIn, stage)
        assertTrue(store.state.value.isReady)
        assertEquals(listOf(Device("phone", "Pixel", certIssuedAtMs = 1)), store.state.value.devices)
    }

    @Test
    fun goesBackToTheChoiceWhenTheApprovalEndsWithoutIt() {
        store.reset("ana")
        store.receiveAuth(event("auth.ready", """"userId":"ana","enrolled":false"""))
        store.requestDeviceApproval()

        store.receiveDevices(event("devices.link.selfResolved", """"outcome":"expired""""))
        assertEquals(SignInStage.TrustingDevice(DeviceTrust.Choose), stage)
    }

    @Test
    fun ignoresTheReadinessOfAnotherAccount() {
        store.reset("ana")
        store.receiveAuth(event("auth.ready", """"userId":"bo","enrolled":true"""))

        assertEquals(listOf(TestWords.otherAccount), notices)
        assertFalse(store.state.value.authResolved)
    }

    @Test
    fun keepsAFailedSignInOnTheStepWhereItFailed() {
        store.signIn()
        store.receiveAuth(event("auth.browser", """"requestId":"r","url":"https://id.kodosi.com/login""""))

        val shown = store.absorb(event("auth.error", """"message":"The sign-in page did not answer.""""), "nope")
        assertTrue(shown)
        assertEquals(SignInStage.Failed("nope", 1), stage)
    }

    @Test
    fun tellsWhenTheSignInEnded() {
        store.receiveAuth(event("auth.required", """"reason":"expired""""))

        assertEquals(listOf(TestWords.signInEnded), notices)
        assertTrue(store.state.value.needsOnboarding)
    }

    @Test
    fun followsWhetherKodosiReachesTheServer() {
        store.receiveConnection(event("connection.state", """"online":false"""))
        assertFalse(store.state.value.online)

        store.receiveConnection(event("connection.state", """"online":true"""))
        assertTrue(store.state.value.online)
    }

    @Test
    fun anotherAccountStartsOnline() {
        store.reset("ana")
        store.receiveConnection(event("connection.state", """"online":false"""))

        store.reset("bo")
        assertTrue(store.state.value.online)
    }
}
