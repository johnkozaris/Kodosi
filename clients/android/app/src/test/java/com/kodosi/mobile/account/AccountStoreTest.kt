package com.kodosi.mobile.account

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.TestWords
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class AccountStoreTest {
    private val sent = SentCommands()
    private val notices = mutableListOf<String>()
    private val store = AccountStore(sent.sink, TestWords) {
        "el"
    }.also { it.notify = { message -> notices += message } }

    private val stage get() = store.state.value.stage

    @Test
    fun opensOnlyPagesThatAreSafeToOpen() {
        store.signIn()
        store.receiveAuth(event("auth.browser", """"requestId":"r","url":"http://192.168.1.4/login""""))
        assertEquals(SignInStage.AwaitingBrowser, stage)
        assertNull(store.state.value.signInPage)

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
    fun ignoresTheReadinessOfAnotherAccount() {
        store.reset("ana")
        store.receiveAuth(event("auth.ready", """"userId":"bo","enrolled":true"""))

        assertEquals(listOf(TestWords.otherAccount), notices)
        assertFalse(store.state.value.authResolved)
    }
}
