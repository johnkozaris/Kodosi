package com.kodosi.mobile

import android.app.Application
import com.kodosi.mobile.account.SignInStage
import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.event
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AppModelTest {
    @Before
    fun mainDispatcher() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun resetDispatcher() = Dispatchers.resetMain()

    @Test
    fun aRestoreFailureWaitsForRuntimeAuthStateWithoutReplacingTheApp() {
        val model = AppModel(RuntimeEnvironment.getApplication())
        model.receive(event("""{"type":"system.ready","accountUserId":"ana","accountEpoch":1}"""))
        model.receive(
            event(
                """{"type":"auth.restoring","accountUserId":"ana","accountEpoch":1,"reason":"No connection."}"""
            )
        )
        model.receive(
            event(
                """{"type":"auth.error","accountUserId":"ana","accountEpoch":1,"operation":"auth.refresh","message":"No connection."}"""
            )
        )

        assertEquals(AppModel.Phase.Ready, model.phase.value)
        assertEquals("No connection.", model.account.state.value.restoring)
        assertFalse(model.account.state.value.authResolved)
        model.receive(
            event("""{"type":"auth.ready","accountUserId":"ana","accountEpoch":1,"userId":"ana","enrolled":true}""")
        )
        assertTrue(model.account.state.value.isReady)
        assertEquals(null, model.account.state.value.restoring)
    }

    @Test
    fun aSignInFailureStaysInOnboardingAfterTheAccountWasResolved() {
        val model = AppModel(RuntimeEnvironment.getApplication())
        model.receive(event("system.ready", ""))
        model.receive(event("auth.required", ""))
        model.account.signIn()
        model.receive(event("auth.error", """"operation":"auth.login.start","message":"No connection.""""))

        assertEquals(AppModel.Phase.Ready, model.phase.value)
        assertEquals(SignInStage.Failed("No connection.", 0), model.account.state.value.stage)
    }

    @Test
    fun aRefreshFailureDoesNotReplaceAnAlreadyOpenAccount() {
        val model = AppModel(RuntimeEnvironment.getApplication())
        model.receive(event("""{"type":"system.ready","accountUserId":"ana","accountEpoch":1}"""))
        model.receive(
            event(
                """{"type":"auth.ready","accountUserId":"ana","accountEpoch":1,"userId":"ana","enrolled":true}"""
            )
        )
        model.receive(
            event(
                """{"type":"auth.error","accountUserId":"ana","accountEpoch":1,"operation":"auth.refresh","message":"No connection."}"""
            )
        )

        assertEquals(AppModel.Phase.Ready, model.phase.value)
        assertEquals(true, model.account.state.value.isReady)
    }

    @Test
    fun aNotificationForANewTerminalWaitsForAFreshCatalog() = runTest {
        val sent = SentCommands()
        val model = AppModel(RuntimeEnvironment.getApplication())
        model.commands.transport = sent.sink.transport
        model.receive(event("""{"type":"system.ready","accountUserId":"ana","accountEpoch":1}"""))
        model.receive(
            event("""{"type":"auth.ready","accountUserId":"ana","accountEpoch":1,"userId":"ana","enrolled":true}""")
        )
        val opening = async(start = CoroutineStart.UNDISPATCHED) {
            model.resolve(AppModel.TerminalTarget("new-terminal", "ana"))
        }
        assertFalse(opening.isCompleted)
        model.receive(
            event(
                """{"type":"sessions.snapshot","accountUserId":"ana","accountEpoch":1,"sessions":[{
                   "id":"new-terminal","incarnationId":"current","name":"Review","kind":"remote",
                   "status":"running","connectionState":"connecting","working":false}]}"""
            )
        )
        val request = sent.last("session.list").string("requestId")
        model.receive(
            event(
                """{"type":"session.result","accountUserId":"ana","accountEpoch":1,
                   "requestId":"$request","operation":"session.list"}"""
            )
        )
        assertTrue(opening.await())
    }

    @Test
    fun aNotificationCannotOpenAnotherAccountsTerminal() = runTest {
        val model = AppModel(RuntimeEnvironment.getApplication())
        assertFalse(model.resolve(AppModel.TerminalTarget("terminal", "another-account")))
    }
}
