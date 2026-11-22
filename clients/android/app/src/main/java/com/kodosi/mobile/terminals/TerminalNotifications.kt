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
import android.os.Bundle
import androidx.core.content.edit
import androidx.core.net.toUri
import com.kodosi.mobile.AppPreferences
import com.kodosi.mobile.MainActivity
import com.kodosi.mobile.R
import com.kodosi.mobile.notifications.PushedTerminal
import com.kodosi.mobile.runtime.RuntimeJson
import kotlinx.serialization.Serializable

class TerminalNotifications(private val application: Context, private val preferences: AppPreferences) {
    @Serializable
    private data class Terminal(
        val id: String,
        val name: String,
        val computer: String?,
        val computerName: String?,
        val hidden: Boolean
    )

    @Serializable
    private data class Cache(val user: String? = null, val terminals: List<Terminal> = emptyList())

    private class Post(val id: String, val status: ProgramStatus, val notification: Notification)

    private val manager = application.getSystemService(NotificationManager::class.java)
    private val saved = application.getSharedPreferences("terminal-notifications", Context.MODE_PRIVATE)

    @Volatile
    private var cache = read<Cache>("cache") ?: Cache()
    private var posted = read<Map<String, ProgramStatus>>("posted") ?: emptyMap()
    private var attention: Map<String, Boolean>? = null
    private var observed: List<RuntimeSession>? = null

    @Volatile
    var foreground: Boolean = false
    val hasAccount: Boolean get() = cache.user != null
    val allowed: Boolean
        get() = preferences.state.value.notifications && manager.areNotificationsEnabled() &&
            application.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    @Synchronized
    fun showing(id: String?) {
        id?.let { withdraw(setOf(it)) }
    }

    @Synchronized
    fun dismiss() {
        manager.cancelAll()
        record(emptyMap())
    }

    @Synchronized
    fun reset() {
        attention = null
        remember(Cache())
        dismiss()
    }

    @Synchronized
    fun account(user: String?) {
        if (cache.user == user) return
        reset()
        remember(Cache(user))
    }

    @Synchronized
    fun observe(sessions: List<RuntimeSession>, user: String?) {
        account(user)
        if (sessions == observed && attention != null) return
        observed = sessions
        if (user != null) {
            remember(
                Cache(
                    user,
                    sessions.asSequence().filter { it.isOwner }.take(512).map {
                        Terminal(it.id, plain(it.name), it.hostDeviceId, it.hostName?.let(::plain), it.hidden)
                    }.toList()
                )
            )
        }
        val before = attention
        val now = sessions.associate { it.id to it.attention }
        attention = now
        if (before == null || user == null) return
        withdraw(before.keys - now.keys + before.filter { (id, marked) -> marked && now[id] == false }.keys)
    }

    @Synchronized
    fun receive(pushed: List<PushedTerminal>) {
        val current = cache
        val user = current.user ?: return
        val listed = pushed.mapTo(HashSet()) { it.terminal }
        val posts = if (foreground || !allowed) {
            emptyList()
        } else {
            pushed.filter { posted[it.terminal] != it.status }.mapNotNull { entry ->
                val terminal = current.terminals.firstOrNull { it.id == entry.terminal }
                if (terminal?.hidden == true) return@mapNotNull null
                Post(entry.terminal, entry.status, notification(entry, user, terminal))
            }
        }
        if (posts.isNotEmpty()) channels()
        settle({ it in listed }, posts)
    }

    private fun withdraw(ids: Set<String>) {
        if (ids.isNotEmpty()) settle({ it !in ids })
    }

    @SuppressLint("MissingPermission")
    private fun settle(stays: (String) -> Boolean, posts: List<Post> = emptyList()) {
        val active = manager.activeNotifications
        val before = active.filter { it.id == SHOWN }.associate { it.tag to it.notification }
        val gone = (before.keys + posted.keys).filterNot(stays).toSet()
        gone.forEach { manager.cancel(it, SHOWN) }
        posts.forEach { manager.notify(it.id, SHOWN, it.notification) }
        record(posted.filterKeys(stays) + posts.associate { it.id to it.status })
        val shown = before.filterKeys(stays) + posts.associate { it.id to it.notification }
        val groups = shown.values.mapNotNull { it.group?.takeIf { group -> group.startsWith(COMPUTER) } }.toSet()
        active.filter { it.id == SUMMARY && it.tag !in groups }.forEach { manager.cancel(it.tag, SUMMARY) }
        val touched = gone.mapNotNull { before[it]?.group } + posts.mapNotNull { it.notification.group }
        for (group in groups.intersect(touched.toSet())) {
            manager.notify(group, SUMMARY, summary(group, shown.values.filter { it.group == group }))
        }
    }

    private fun notification(entry: PushedTerminal, user: String, terminal: Terminal?): Notification {
        val context = application
        val waiting = entry.status.state == ProgramStatus.State.BLOCKED
        val label = when (entry.status.state) {
            ProgramStatus.State.BLOCKED -> when (entry.status.kind) {
                ProgramStatus.Kind.AUTH -> R.string.sign_key
                ProgramStatus.Kind.QUESTION -> R.string.sign_question
                else -> R.string.sign_hand
            }

            ProgramStatus.State.DONE -> R.string.sign_done

            else -> R.string.sign_failed
        }
        val provider = when (entry.agent) {
            AgentKind.CLAUDE -> R.string.agent_claude
            AgentKind.CODEX -> R.string.agent_codex
            AgentKind.COPILOT -> R.string.agent_copilot
            AgentKind.CURSOR -> R.string.agent_cursor
            AgentKind.OPENCODE -> R.string.agent_opencode
            AgentKind.ANTIGRAVITY -> R.string.agent_antigravity
            AgentKind.SHELL -> null
        }?.let(context::getString)
        val computer = terminal?.computerName
        val group = terminal?.computer?.takeIf { computer != null }?.let { COMPUTER + it }
        val message = listOfNotNull(provider, context.getString(label), computer).joinToString(" \u00B7 ")
        val open = Intent(context, MainActivity::class.java)
            .setData("kodosi://terminal/$user/${entry.terminal}".toUri())
            .putExtra(TERMINAL, entry.terminal)
            .putExtra(ACCOUNT, user)
            .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return Notification.Builder(context, if (waiting) WAITING else ENDED)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(terminal?.name ?: context.getString(R.string.terminal_notification_title))
            .setContentText(message)
            .setStyle(Notification.BigTextStyle().bigText(message))
            .setContentIntent(pending(open))
            .setAutoCancel(true)
            .setVisibility(visibility())
            .addExtras(Bundle().apply { putString(COMPUTER_NAME, computer) })
            // A channel has one sound. Android mutes a group child that leaves the alert to its summary,
            // and a summary of Kodosi never alerts.
            .setGroup(group ?: QUIET.takeUnless { entry.sound })
            .setGroupAlertBehavior(if (entry.sound) Notification.GROUP_ALERT_ALL else Notification.GROUP_ALERT_SUMMARY)
            .build()
    }

    private fun summary(group: String, children: List<Notification>): Notification {
        val computer = children.firstNotNullOfOrNull { it.extras.getString(COMPUTER_NAME) }
        val open = Intent(application, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return Notification.Builder(application, if (children.any { it.channelId == WAITING }) WAITING else ENDED)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(computer)
            .setSubText(computer)
            .setContentIntent(pending(open))
            .setVisibility(visibility())
            .setGroup(group)
            .setGroupSummary(true)
            .setGroupAlertBehavior(Notification.GROUP_ALERT_CHILDREN)
            .build()
    }

    private fun pending(intent: Intent) = PendingIntent.getActivity(
        application,
        0,
        intent,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )

    private fun visibility() = if (preferences.state.value.lock) {
        Notification.VISIBILITY_SECRET
    } else {
        Notification.VISIBILITY_PRIVATE
    }

    private fun channels() {
        manager.createNotificationChannels(
            listOf(
                NotificationChannel(
                    WAITING,
                    application.getString(R.string.notifications_waiting),
                    NotificationManager.IMPORTANCE_HIGH
                ).apply { description = application.getString(R.string.notifications_waiting_description) },
                NotificationChannel(
                    ENDED,
                    application.getString(R.string.notifications_ended),
                    NotificationManager.IMPORTANCE_LOW
                ).apply { description = application.getString(R.string.notifications_ended_description) }
            )
        )
    }

    private fun record(next: Map<String, ProgramStatus>) {
        if (posted == next) return
        posted = next
        saved.edit { putString("posted", RuntimeJson.encodeToString(next)) }
    }

    private fun remember(next: Cache) {
        if (cache == next) return
        cache = next
        saved.edit { putString("cache", RuntimeJson.encodeToString(next)) }
    }

    private inline fun <reified T> read(key: String): T? = runCatching {
        saved.getString(key, null)?.let { RuntimeJson.decodeFromString<T>(it) }
    }.getOrNull()

    private fun plain(text: String): String = text.filterNot {
        it.isISOControl() || Character.getType(it) == Character.FORMAT.toInt()
    }.take(512)

    companion object {
        private const val WAITING = "terminal-waiting"
        private const val ENDED = "terminal-ended"
        private const val SHOWN = 0
        private const val SUMMARY = 1
        private const val COMPUTER = "computer:"
        private const val QUIET = "quiet"
        private const val COMPUTER_NAME = "com.kodosi.mobile.COMPUTER_NAME"
        const val TERMINAL = "com.kodosi.mobile.TERMINAL"
        const val ACCOUNT = "com.kodosi.mobile.ACCOUNT"
    }
}
