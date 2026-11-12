package com.kodosi.mobile

import android.app.Application
import com.google.firebase.messaging.FirebaseMessaging
import com.kodosi.mobile.account.AccountStore
import com.kodosi.mobile.account.AppLock
import com.kodosi.mobile.canvas.CanvasStore
import com.kodosi.mobile.canvas.DocumentPages
import com.kodosi.mobile.dosi.DosiStore
import com.kodosi.mobile.notifications.FirebaseConfiguration
import com.kodosi.mobile.notifications.PushRegistration
import com.kodosi.mobile.people.PeopleStore
import com.kodosi.mobile.projects.ProjectStore
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.InvalidEvent
import com.kodosi.mobile.runtime.RuntimeEnvironment
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeHandle
import com.kodosi.mobile.terminals.TerminalConnection
import com.kodosi.mobile.terminals.TerminalNotifications
import com.kodosi.mobile.terminals.TerminalStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.SerializationException

/** The app's state: the runtime, its events and the stores that they fill. */
class AppModel(
    private val application: Application,
    val preferences: AppPreferences = AppPreferences(application),
    val notifications: TerminalNotifications = TerminalNotifications(application, preferences)
) {
    sealed interface Phase {
        data object Launching : Phase

        data object Ready : Phase

        data class Failed(val message: String) : Phase
    }

    private val scope = MainScope()
    val words = AndroidWords(application)
    val commands = CommandSink(words)
    val account = AccountStore(commands, words) { AppLanguage.tag(application) }
    val terminals = TerminalStore(commands)
    val push = PushRegistration(
        commands,
        scope,
        token = { FirebaseMessaging.getInstance().token.await() },
        forgetToken = { FirebaseMessaging.getInstance().deleteToken().await() }
    )
    data class TerminalTarget(val id: String, val account: String)
    val terminalTarget = MutableStateFlow<TerminalTarget?>(null)
    val friendInvite = MutableStateFlow<String?>(null)
    val projects = ProjectStore(commands)
    val people = PeopleStore(commands)
    val dosi = DosiStore(commands)
    val canvas = CanvasStore(commands, words)
    internal val documents = DocumentPages(application, canvas, scope, ::notify)
    val lock = AppLock(preferences)
    private val mutablePhase = MutableStateFlow<Phase>(Phase.Launching)
    val phase: StateFlow<Phase> = mutablePhase
    private val mutableNotices = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val notices: SharedFlow<String> = mutableNotices
    private var runtime: RuntimeHandle? = null
    private var consuming: Job? = null
    private var stopping: Job? = null
    private var accountKnown = false
    private var accountEpoch = 0L

    init {
        commands.onFailure = ::notify
        account.notify = ::notify
    }

    fun start() {
        if (runtime != null || phase.value is Phase.Failed) return
        val handle = RuntimeHandle()
        runtime = handle
        val previous = stopping
        consuming = scope.launch {
            previous?.join()
            val outcome = withContext(Dispatchers.IO) {
                try {
                    RuntimeEnvironment.install(application)
                } catch (failure: Exception) {
                    return@withContext RuntimeHandle.Start.Failed(words.storage)
                }
                handle.start(application, AppLanguage.tag(application))
            }
            when (outcome) {
                RuntimeHandle.Start.Started -> {
                    commands.transport = handle::send
                    consume(handle)
                }

                RuntimeHandle.Start.Incompatible -> fail(words.damaged)

                is RuntimeHandle.Start.Failed -> fail(outcome.message ?: words.couldNotStart)
            }
        }
    }

    fun retry() {
        stop()
        mutablePhase.value = Phase.Launching
        accountKnown = false
        accountEpoch = 0
        commands.setAccount(null, 0)
        account.reset(null)
        terminals.reset()
        notifications.reset()
        projects.reset(null)
        people.reset()
        dosi.reset()
        documents.reset()
        canvas.reset()
        start()
    }

    fun languageChanged() {
        if (runtime != null) RuntimeHandle.setLanguage(AppLanguage.tag(application))
    }

    /** Asks again for what may have changed while the app was away. */
    fun refresh() {
        if (phase.value != Phase.Ready) return
        terminals.list()
        if (account.state.value.userId == null) return
        commands.perform("auth.refresh")
        commands.perform("devices.refresh")
        people.refresh()
        projects.refresh()
    }

    fun updatePush(force: Boolean = false): Long? {
        val current = account.state.value
        if (!current.authResolved) return null
        if (!preferences.state.value.notifications) notifications.dismiss()
        val user = current.userId.takeIf { current.isReady }
        notifications.account(user)
        if (!FirebaseConfiguration.available) return null
        FirebaseMessaging.getInstance().isAutoInitEnabled = user != null && notifications.allowed
        return push.update(user, commands.accountEpoch, notifications.allowed, force)
    }

    internal suspend fun synchronizePush(): Boolean = withContext(Dispatchers.Main) {
        if (phase.value is Phase.Failed) retry() else start()
        refresh()
        withTimeoutOrNull(45_000) {
            val restored = combine(phase, account.state) { phase, account ->
                when {
                    phase is Phase.Failed -> false
                    account.authResolved -> true
                    else -> null
                }
            }.filterNotNull().first()
            if (!restored) return@withTimeoutOrNull false
            updatePush(force = true)?.let { push.await(it) } ?: true
        } ?: false
    }

    fun connection(sessionId: String, scope: CoroutineScope): TerminalConnection? {
        val handle = runtime ?: return null
        return TerminalConnection(sessionId, handle, commands, terminals, scope, words)
    }

    suspend fun resolve(target: TerminalTarget): Boolean {
        if (!account.state.value.isReady || account.state.value.userId != target.account) return false
        if (terminals.session(target.id) == null) commands.request("session.list")
        return account.state.value.isReady && account.state.value.userId == target.account &&
            terminals.session(target.id) != null
    }

    private suspend fun consume(handle: RuntimeHandle) {
        for (bytes in handle.events) {
            try {
                receive(RuntimeEvent.parse(bytes))
            } catch (failure: InvalidEvent) {
                fail(words.unreadable(failure.message.orEmpty()))
                return
            } catch (failure: SerializationException) {
                fail(words.unreadable(failure.message.orEmpty()))
                return
            } catch (failure: IllegalArgumentException) {
                fail(words.unreadable(failure.message.orEmpty()))
                return
            }
        }
        if (handle.overflowed) fail(words.fellBehind)
    }

    private fun acceptAccount(event: RuntimeEvent): Boolean {
        val establishes = event.type in setOf("auth.ready", "auth.required", "system.ready")
        if (!accountKnown || event.accountEpoch > accountEpoch) {
            if (!establishes) return false
            accountKnown = true
            accountEpoch = event.accountEpoch
            if (event.type != "system.ready" || event.accountUserId != null) preferences.account(event.accountUserId)
            account.reset(event.accountUserId)
            terminals.reset()
            notifications.reset()
            projects.reset(event.accountUserId)
            people.reset()
            dosi.reset()
            documents.reset()
            canvas.reset()
            commands.setAccount(event.accountUserId, event.accountEpoch)
        }
        return event.accountEpoch == accountEpoch && event.accountUserId == account.state.value.userId
    }

    fun receive(event: RuntimeEvent) {
        if (!acceptAccount(event)) return
        val delivery = commands.receive(event)
        if (event.type.endsWith(".error")) {
            terminals.absorb(event)
            if (event.type == "document.error") canvas.receive(event)
            if (delivery != CommandSink.Delivery.APP) return
            if (event.failedRefresh) return
            val message = event.string("message") ?: words.didNotWork
            if (event.type == "auth.error" && !account.state.value.authResolved) {
                fail(message)
            } else if (!account.absorb(event, message)) {
                notify(message)
            }
            return
        }
        when (event.type.substringBefore('.')) {
            "system" -> if (event.type == "system.ready") {
                mutablePhase.value = Phase.Ready
                refresh()
            }

            "auth" -> {
                account.receiveAuth(event)
                updatePush(force = event.type == "auth.ready")
                if (event.type == "auth.ready" || event.type == "auth.required") terminals.list()
                if (event.type == "auth.ready") {
                    commands.perform("devices.refresh")
                    people.refresh()
                    projects.refresh()
                }
            }

            "devices" -> {
                account.receiveDevices(event)
                updatePush()
            }

            "connection" -> account.receiveConnection(event)

            "sessions", "session" -> {
                terminals.receive(event)
                canvas.observe(terminals.state.value.sessions)
                notifications.observe(
                    terminals.state.value.sessions,
                    account.state.value.userId.takeIf { account.state.value.isReady }
                )
            }

            "friends" -> people.receive(event)

            "project", "projects" -> projects.receive(event)

            "dosi" -> dosi.receive(event)

            "document" -> canvas.receive(event)
        }
    }

    private fun fail(message: String) {
        if (phase.value == Phase.Failed(message)) return
        stop()
        mutablePhase.value = Phase.Failed(message)
    }

    /** Stops the runtime away from the main thread; the next start waits for it. */
    private fun stop() {
        commands.cancelPending()
        commands.transport = null
        val handle = runtime ?: return
        runtime = null
        val consumer = consuming
        consuming = null
        consumer?.cancel()
        val previous = stopping
        stopping = scope.launch(Dispatchers.IO) {
            previous?.join()
            consumer?.join()
            handle.stop()
        }
    }

    fun notify(message: String) {
        if (message.isNotEmpty()) mutableNotices.tryEmit(message)
    }

    suspend fun runAction(work: suspend () -> Unit) {
        try {
            work()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Exception) {
            notify(failure.localizedMessage ?: words.didNotWork)
        }
    }
}
