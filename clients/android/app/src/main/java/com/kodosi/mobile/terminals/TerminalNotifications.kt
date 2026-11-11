package com.kodosi.mobile.terminals

import android.Manifest
import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.content.edit
import androidx.core.net.toUri
import com.kodosi.mobile.AppPreferences
import com.kodosi.mobile.MainActivity
import com.kodosi.mobile.R
import com.kodosi.mobile.notifications.TerminalAlert
import com.kodosi.mobile.runtime.RuntimeJson
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString

class TerminalNotifications(private val application: Context, private val preferences: AppPreferences) {
    @Serializable
    private data class Terminal(val id: String, val name: String, val computerName: String?, val hidden: Boolean)

    @Serializable
    private data class Cache(val user: String? = null, val terminals: List<Terminal> = emptyList())

    private val manager = application.getSystemService(NotificationManager::class.java)
    private val saved = application.getSharedPreferences("terminal-notifications", Context.MODE_PRIVATE)

    @Volatile
    private var cache = runCatching {
        RuntimeJson.decodeFromString<Cache>(saved.getString("cache", "{}").orEmpty())
    }.getOrDefault(Cache())
    private var previous: Map<String, RuntimeSession>? = null

    @Volatile
    var foreground: Boolean = false
    val hasAccount: Boolean get() = cache.user != null
    val allowed: Boolean
        get() = preferences.state.value.notifications && manager.areNotificationsEnabled() &&
            application.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    fun showing(id: String?) {
        id?.let { manager.cancel(it, 0) }
    }

    fun dismiss() {
        manager.cancelAll()
    }

    fun reset() {
        previous = null
        remember(Cache())
        manager.cancelAll()
    }

    fun account(user: String?) {
        if (cache.user == user) return
        reset()
        remember(Cache(user))
    }

    fun observe(sessions: List<RuntimeSession>, user: String?) {
        account(user)
        if (user != null) {
            remember(
                Cache(
                    user,
                    sessions.asSequence().filter { it.isOwner }.take(512).map {
                        Terminal(it.id, plain(it.name), it.hostName?.let(::plain), it.hidden)
                    }.toList()
                )
            )
        }
        val old = previous
        previous = sessions.associateBy { it.id }
        if (old == null || user == null) return
        old.keys.filter { id -> sessions.none { it.id == id } }.forEach { manager.cancel(it, 0) }
        for (session in sessions) {
            if (session.hidden || (!session.isOwner && !session.canControl)) continue
            val before = old[session.id]
            if (before?.incarnationId == session.incarnationId &&
                before.waitState == session.waitState &&
                before.programStatus?.kind == session.programStatus?.kind &&
                before.activity == session.activity
            ) {
                continue
            }
            val waiting = session.waitState
            if (waiting == null) {
                manager.cancel(session.id, 0)
                continue
            }
            show(
                session.id,
                user,
                plain(session.name),
                session.hostName?.let(::plain),
                session.agent,
                ProgramStatus(waiting, session.programStatus?.kind),
                session.activity?.let(::plain)
            )
        }
    }

    fun receive(alert: TerminalAlert) {
        val current = cache
        val user = current.user ?: return
        val terminal = current.terminals.firstOrNull { it.id == alert.terminal }
        if (terminal?.hidden == true) return
        show(alert.terminal, user, terminal?.name, terminal?.computerName, alert.agent, alert.status)
    }

    @SuppressLint("MissingPermission")
    private fun show(
        id: String,
        user: String,
        name: String?,
        computer: String?,
        agent: AgentKind,
        status: ProgramStatus,
        activity: String? = null
    ) {
        if (foreground || !allowed) return
        val context = application
        val waiting = status.state == ProgramStatus.State.BLOCKED
        val channel = if (waiting) WAITING else UPDATES
        manager.createNotificationChannel(
            NotificationChannel(
                channel,
                context.getString(if (waiting) R.string.notifications_waiting else R.string.terminals),
                if (waiting) NotificationManager.IMPORTANCE_HIGH else NotificationManager.IMPORTANCE_DEFAULT
            ).apply { description = context.getString(R.string.notifications_description) }
        )
        val label = when (status.state) {
            ProgramStatus.State.BLOCKED -> when (status.kind) {
                ProgramStatus.Kind.AUTH -> R.string.sign_key
                ProgramStatus.Kind.QUESTION -> R.string.sign_question
                else -> R.string.sign_hand
            }

            ProgramStatus.State.DONE -> R.string.sign_done

            ProgramStatus.State.ERROR -> R.string.sign_failed

            else -> return
        }
        val provider = when (agent) {
            AgentKind.CLAUDE -> R.string.agent_claude
            AgentKind.CODEX -> R.string.agent_codex
            AgentKind.COPILOT -> R.string.agent_copilot
            AgentKind.CURSOR -> R.string.agent_cursor
            AgentKind.GEMINI -> R.string.agent_gemini
            AgentKind.OPENCODE -> R.string.agent_opencode
            AgentKind.SHELL -> null
        }?.let(context::getString)
        val message = listOfNotNull(provider, activity ?: context.getString(label), computer).joinToString(" \u00B7 ")
        val open = Intent(context, MainActivity::class.java)
            .setData("kodosi://terminal/$user/$id".toUri())
            .putExtra(TERMINAL, id)
            .putExtra(ACCOUNT, user)
            .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val pending = PendingIntent.getActivity(
            context,
            0,
            open,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val notification = Notification.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(name ?: context.getString(R.string.terminal_notification_title))
            .setContentText(message)
            .setStyle(Notification.BigTextStyle().bigText(message))
            .setContentIntent(pending)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setVisibility(
                if (preferences.state.value.lock) {
                    Notification.VISIBILITY_SECRET
                } else {
                    Notification.VISIBILITY_PRIVATE
                }
            )
            .build()
        manager.notify(id, 0, notification)
    }

    private fun remember(next: Cache) {
        if (cache == next) return
        cache = next
        saved.edit { putString("cache", RuntimeJson.encodeToString(next)) }
    }

    private fun plain(text: String): String = text.filterNot {
        it.isISOControl() || Character.getType(it) == Character.FORMAT.toInt()
    }.take(512)

    companion object {
        private const val WAITING = "terminal-waiting"
        private const val UPDATES = "terminal-updates"
        const val TERMINAL = "com.kodosi.mobile.TERMINAL"
        const val ACCOUNT = "com.kodosi.mobile.ACCOUNT"
    }
}
