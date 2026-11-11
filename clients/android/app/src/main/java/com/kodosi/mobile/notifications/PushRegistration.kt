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
    private val token: suspend () -> String,
    private val forgetToken: suspend () -> Unit
) {
    private data class Target(val user: String?, val epoch: Long, val enabled: Boolean)
    private data class Outcome(val version: Long, val issue: String? = null)

    private var target: Target? = null
    private var version = 0L
    private var completed = 0L
    private var worker: Job? = null
    private val outcome = MutableStateFlow(Outcome(0))
    private val mutableIssue = MutableStateFlow<String?>(null)
    val issue: StateFlow<String?> = mutableIssue

    fun update(user: String?, epoch: Long, enabled: Boolean, force: Boolean = false): Long {
        val next = Target(user, epoch, enabled && user != null)
        if (next != target || force) {
            target = next
            version += 1
            mutableIssue.value = null
        }
        if (worker?.isActive != true && completed != version) worker = scope.launch { synchronize() }
        return version
    }

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
                    val value = token()
                    if (pending != version) continue
                    commands.request("push.register", fields("platform" to "fcm", "token" to value))
                } else {
                    try {
                        if (next.user != null) commands.request("push.unregister")
                    } finally {
                        forgetToken()
                    }
                }
                completed = pending
                mutableIssue.value = null
                outcome.value = Outcome(pending)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                if (pending == version) {
                    val issue = failure.localizedMessage ?: failure.javaClass.simpleName
                    mutableIssue.value = issue
                    outcome.value = Outcome(pending, issue)
                    if (pending == version) return
                }
            }
            if (pending == version) return
        }
    }
}
