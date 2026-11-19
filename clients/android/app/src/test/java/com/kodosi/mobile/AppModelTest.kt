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
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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
    fun aJoinCodeThatOpensTheAppAsksOnceThisDeviceNeedsTrust() {
        val sent = SentCommands()
        val model = AppModel(RuntimeEnvironment.getApplication())
        model.commands.transport = sent.sink.transport
        model.deviceJoin.value = "kodosi://device/join/ana#secret"
        model.receive(event("""{"type":"system.ready","accountUserId":"ana","accountEpoch":1}"""))
        assertFalse("devices.link.startSelf" in sent.types)

        model.receive(
            event("""{"type":"auth.ready","accountUserId":"ana","accountEpoch":1,"userId":"ana","enrolled":false}""")
        )

        assertEquals("kodosi://device/join/ana#secret", sent.last("devices.link.startSelf").string("code"))
        assertNull(model.deviceJoin.value)
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
    fun theRuntimeHearsWhereThePersonIsAndWhereTheAppIsAfterEachReadyAndOnEachChange() {
        val sent = SentCommands()
        val model = AppModel(RuntimeEnvironment.getApplication())
        model.commands.transport = sent.sink.transport
        fun states(type: String) = sent.commands.filter { it.string("type") == type }.map { it.string("state") }

        model.present(true)
        assertEquals(emptyList<String>(), states("app.presence"))
        model.receive(event("system.ready", """"awaySeconds":180"""))
        assertEquals(listOf("app.presence", "app.lifecycle", "session.list"), sent.types)
        assertEquals(listOf("kodosi"), states("app.presence"))
        assertEquals(listOf("foreground"), states("app.lifecycle"))

        model.present(true)
        model.present(false)
        model.present(false)
        assertEquals(listOf("kodosi", "away"), states("app.presence"))
        assertEquals(listOf("foreground", "background"), states("app.lifecycle"))
        model.receive(event("system.ready", """"awaySeconds":180"""))
        assertEquals(listOf("kodosi", "away", "away"), states("app.presence"))
        assertEquals(listOf("foreground", "background", "background"), states("app.lifecycle"))
        model.present(true)
        assertEquals(listOf("kodosi", "away", "away", "kodosi"), states("app.presence"))
        assertEquals(listOf("foreground", "background", "background", "foreground"), states("app.lifecycle"))
    }

    @Test
    fun aNotificationForANewTerminalWaitsForAFreshCatalog() = runTest {
        val sent = SentCommands()
        val model = signedIn(sent)
        val opening = async(start = CoroutineStart.UNDISPATCHED) {
            model.resolve(AppModel.TerminalTarget("new-terminal", "ana"))
        }
        assertFalse(opening.isCompleted)
        model.receive(snapshot(terminal("new-terminal")))
        listed(model, sent)
        assertEquals("new-terminal", opening.await()?.id)
    }

    @Test
    fun aNotificationFollowsOnlyYourOwnRestartOfItsTerminal() = runTest {
        val sent = SentCommands()
        val model = signedIn(sent)
        val opening = async(start = CoroutineStart.UNDISPATCHED) {
            model.resolve(AppModel.TerminalTarget("ended", "ana"))
        }
        model.receive(
            snapshot(
                terminal("theirs", owner = false, restoredFrom = "ended"),
                terminal("again", restoredFrom = "ended")
            )
        )
        listed(model, sent)
        assertEquals("again", opening.await()?.id)
    }

    @Test
    fun aNotificationOfAGoneOrUnreachableTerminalOpensNothingAndSaysNothing() = runTest {
        val sent = SentCommands()
        val model = signedIn(sent)
        val notices = mutableListOf<String>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { model.notices.toList(notices) }
        val gone = async(start = CoroutineStart.UNDISPATCHED) {
            model.resolve(AppModel.TerminalTarget("gone", "ana"))
        }
        model.receive(snapshot(terminal("hidden", hidden = true)))
        listed(model, sent)
        assertNull(gone.await())
        val unreachable = async(start = CoroutineStart.UNDISPATCHED) {
            model.resolve(AppModel.TerminalTarget("hidden", "ana"))
        }
        val request = sent.last("session.list").string("requestId")
        model.receive(
            event(
                """{"type":"session.error","accountUserId":"ana","accountEpoch":1,
                   "requestId":"$request","operation":"session.list","message":"No connection."}"""
            )
        )
        assertNull(unreachable.await())
        assertEquals(emptyList<String>(), notices)
    }

    @Test
    fun aNotificationCannotOpenAnotherAccountsTerminal() = runTest {
        val model = signedIn(SentCommands())
        model.receive(snapshot(terminal("terminal")))
        assertNull(model.resolve(AppModel.TerminalTarget("terminal", "another-account")))
    }

    private fun signedIn(sent: SentCommands) = AppModel(RuntimeEnvironment.getApplication()).also { model ->
        model.commands.transport = sent.sink.transport
        model.receive(event("""{"type":"system.ready","accountUserId":"ana","accountEpoch":1}"""))
        model.receive(
            event("""{"type":"auth.ready","accountUserId":"ana","accountEpoch":1,"userId":"ana","enrolled":true}""")
        )
    }

    private fun snapshot(vararg terminals: String) = terminals.joinToString(",").let { sessions ->
        event("""{"type":"sessions.snapshot","accountUserId":"ana","accountEpoch":1,"sessions":[$sessions]}""")
    }

    private fun terminal(id: String, owner: Boolean = true, restoredFrom: String? = null, hidden: Boolean = false) =
        """{"id":"$id","incarnationId":"1","name":"Review","kind":"remote","status":"running",
           "connectionState":"connecting","working":false,"isOwner":$owner,"hidden":$hidden,
           "restoredFrom":${restoredFrom?.let { "\"$it\"" } ?: "null"}}"""

    private fun listed(model: AppModel, sent: SentCommands) {
        val request = sent.last("session.list").string("requestId")
        model.receive(
            event(
                """{"type":"session.result","accountUserId":"ana","accountEpoch":1,
                   "requestId":"$request","operation":"session.list"}"""
            )
        )
    }
}
