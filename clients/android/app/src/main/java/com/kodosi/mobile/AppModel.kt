package com.kodosi.mobile

import android.app.Application
import com.kodosi.mobile.account.AccountStore
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.InvalidEvent
import com.kodosi.mobile.runtime.RuntimeEnvironment
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeHandle
import com.kodosi.mobile.terminals.TerminalConnection
import com.kodosi.mobile.terminals.TerminalStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/** The app's state: the runtime, its events and the stores that they fill. */
class AppModel(private val application: Application) {
    sealed interface Phase {
        data object Launching : Phase

        data object Ready : Phase

        data class Failed(val message: String) : Phase
    }

    val words = AndroidWords(application)
    val commands = CommandSink(words)
    val account = AccountStore(commands, words) { AppLanguage.tag(application) }
    val terminals = TerminalStore(commands)
    private val mutablePhase = MutableStateFlow<Phase>(Phase.Launching)
    val phase: StateFlow<Phase> = mutablePhase
    private val mutableNotices = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val notices: SharedFlow<String> = mutableNotices
    private val scope = MainScope()
    private var runtime: RuntimeHandle? = null
    private var stopping: Job? = null
    private var accountKnown = false
    private var accountEpoch = 0L

    init {
        commands.onFailure = ::notify
        account.notify = ::notify
    }

    fun start() {
        if (runtime != null || phase.value is Phase.Failed) return
        try {
            RuntimeEnvironment.install(application)
        } catch (failure: Exception) {
            mutablePhase.value = Phase.Failed(words.storage)
            return
        }
        val handle = RuntimeHandle()
        runtime = handle
        val previous = stopping
        scope.launch {
            previous?.join()
            val outcome = withContext(Dispatchers.IO) { handle.start(application, AppLanguage.tag(application)) }
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
    }

    fun connection(sessionId: String, scope: CoroutineScope): TerminalConnection? {
        val handle = runtime ?: return null
        return TerminalConnection(sessionId, handle, commands, terminals, scope, words)
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
            account.reset(event.accountUserId)
            terminals.reset()
            commands.setAccount(event.accountUserId, event.accountEpoch)
        }
        return event.accountEpoch == accountEpoch && event.accountUserId == account.state.value.userId
    }

    fun receive(event: RuntimeEvent) {
        if (!acceptAccount(event)) return
        val delivery = commands.receive(event)
        if (event.type.endsWith(".error")) {
            terminals.absorb(event)
            if (delivery != CommandSink.Delivery.APP) return
            val message = event.string("message") ?: words.didNotWork
            if (event.type == "system.error" && !account.state.value.authResolved) {
                fail(message)
            } else if (!account.absorb(event, message) && !event.failedRefresh) {
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
                if (event.type == "auth.ready" || event.type == "auth.required") terminals.list()
                if (event.type == "auth.ready") commands.perform("devices.refresh")
            }

            "devices" -> account.receiveDevices(event)

            "connection" -> account.receiveConnection(event)

            "sessions", "session" -> terminals.receive(event)
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
        val previous = stopping
        stopping = scope.launch(Dispatchers.IO) {
            previous?.join()
            handle.stop()
        }
    }

    private fun notify(message: String) {
        if (message.isNotEmpty()) mutableNotices.tryEmit(message)
    }
}
