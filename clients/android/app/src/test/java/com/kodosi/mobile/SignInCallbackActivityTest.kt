package com.kodosi.mobile

import android.content.Intent
import android.net.Uri
import com.kodosi.mobile.account.SignInStage
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = KodosiApplication::class)
class SignInCallbackActivityTest {
    @Before
    fun mainDispatcher() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun resetDispatcher() = Dispatchers.resetMain()

    @Test
    fun forwardsTheBrowserResponseAndReturnsToTheExistingApp() {
        val model = (RuntimeEnvironment.getApplication() as KodosiApplication).model
        val sent = SentCommands()
        model.commands.transport = sent.sink.transport
        model.account.signIn()
        model.account.receiveAuth(event("auth.browser", """"url":"https://id.kodosi.com/login""""))
        val answer = "com.kodosi:/auth?code=example"

        val activity = Robolectric.buildActivity(
            SignInCallbackActivity::class.java,
            Intent(Intent.ACTION_VIEW, Uri.parse(answer))
        ).create().get()

        assertEquals(SignInStage.Finalizing, model.account.state.value.stage)
        assertEquals(answer, sent.last("auth.login.finish").string("url"))
        val returning = shadowOf(activity).nextStartedActivity
        assertEquals(MainActivity::class.java.name, returning.component?.className)
        assertTrue(returning.flags and Intent.FLAG_ACTIVITY_CLEAR_TOP != 0)
        assertTrue(activity.isFinishing)
    }

    @Test
    fun ignoresAnUnrelatedLinkWithoutChangingThePendingSignIn() {
        val model = (RuntimeEnvironment.getApplication() as KodosiApplication).model
        val sent = SentCommands()
        model.commands.transport = sent.sink.transport
        model.account.signIn()
        model.account.receiveAuth(event("auth.browser", """"url":"https://id.kodosi.com/login""""))

        Robolectric.buildActivity(
            SignInCallbackActivity::class.java,
            Intent(Intent.ACTION_VIEW, Uri.parse("https://example.invalid/"))
        ).create()

        assertEquals(SignInStage.AwaitingBrowser("https://id.kodosi.com/login"), model.account.state.value.stage)
        assertFalse("auth.login.finish" in sent.types)
    }
}
