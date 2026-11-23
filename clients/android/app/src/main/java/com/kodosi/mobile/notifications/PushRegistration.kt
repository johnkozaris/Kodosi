package com.kodosi.mobile.notifications

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.fields
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

class PushRegistration(
    private val commands: CommandSink,
    private val scope: CoroutineScope,
    private val installation: suspend () -> String,
    private val register: suspend () -> Unit,
    private val unregister: suspend () -> Unit
) {
    private data class Target(val user: String?, val epoch: Long, val enabled: Boolean)
    private data class Outcome(val version: Long, val issue: String? = null)

    private var target: Target? = null
    private var version = 0L
    private var completed = 0L
    private var worker: Job? = null

    @Volatile
    private var registering: String? = null
    private val outcome = MutableStateFlow(Outcome(0))
    val issue: StateFlow<String?>
        field = MutableStateFlow<String?>(null)

    fun update(user: String?, epoch: Long, enabled: Boolean, force: Boolean = false): Long {
        val next = Target(user, epoch, enabled && user != null)
        if (next != target || force) {
            target = next
            version += 1
            issue.value = null
        }
        if (worker?.isActive != true && completed != version) worker = scope.launch { synchronize() }
        return version
    }

    fun registers(installation: String): Boolean = installation == registering

    suspend fun await(version: Long): Boolean = outcome.first { it.version >= version }.issue == null

    private suspend fun synchronize() {
        while (true) {
            val next = target ?: return
            val pending = version
            if (next.user != null &&
                (next.user != commands.accountUserId || next.epoch != commands.accountEpoch)
            ) {
                return
            }
            try {
                if (next.enabled) {
                    val id = installation()
                    registering = id
                    register()
                    if (pending != version) continue
                    commands.request("push.register", fields("platform" to "fcm", "token" to id))
                } else {
                    registering = null
                    try {
                        if (next.user != null) commands.request("push.unregister")
                    } finally {
                        unregister()
                    }
                }
                completed = pending
                issue.value = null
                outcome.value = Outcome(pending)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                registering = null
                if (pending == version) {
                    val message = failure.localizedMessage ?: failure.javaClass.simpleName
                    issue.value = message
                    outcome.value = Outcome(pending, message)
                    if (pending == version) return
                }
            }
            if (pending == version) return
        }
    }
}
