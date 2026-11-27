package com.kodosi.mobile.chat

import com.kodosi.mobile.canvas.CanvasStore
import com.kodosi.mobile.canvas.DocumentLink
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.JsonObject

// answers tracks locally submitted responses until the host clears the ask or sends a new snapshot.
data class ChatView(
    val chat: ChatSnapshot? = null,
    // True once the pipe is open and the initial snapshot has arrived.
    val linked: Boolean = false,
    val failure: String? = null,
    val answers: Map<String, ChatAnswer> = emptyMap()
) {
    val state: ChatState get() = chat?.state ?: ChatState.AWAY
    val items: List<ChatItem> get() = chat?.items.orEmpty()
    val asks: List<ChatAsk> get() = chat?.asks.orEmpty()
    val queued: List<ChatQueued> get() = chat?.queued.orEmpty()
    val tasks: List<ChatTask> get() = chat?.tasks.orEmpty()
    val commands: List<ChatCommand> get() = chat?.commands.orEmpty()
    val settings: List<ChatSetting> get() = chat?.settings.orEmpty()
    val peers: List<ChatPeer> get() = chat?.peers.orEmpty()
    val facts: ChatFacts get() = chat?.facts ?: ChatFacts()
    val now: ChatNow? get() = chat?.now

    val question: ChatAsk? get() = asks.firstOrNull { it.id !in answers }
    val working: Boolean get() = state == ChatState.WORKING || state == ChatState.ASKING
    val canWrite: Boolean get() = linked && state != ChatState.AWAY

    fun item(id: String): ChatItem? = chat?.items?.lastOrNull { it.id == id }

    /** The control of the chat that a draft of only a command's name opens in place of sending it. */
    fun control(draft: String): ChatControl? {
        val text = draft.trim()
        if (!text.startsWith("/")) return null
        val name = text.drop(1)
        return commands.firstOrNull { it.name == name }?.opens
    }
}

class TerminalChat internal constructor(
    private val store: CanvasStore,
    val session: String,
    private val unreadable: (String) -> Unit
) {
    val view: StateFlow<ChatView>
        field = MutableStateFlow(ChatView())

    var draft = ""
    private var link: DocumentLink? = null
    private var watchers = 0

    // Each pipe open starts with a full snapshot; ignore incremental updates until it arrives.
    private var awaitsSnapshot = true

    suspend fun watch() {
        watchers++
        try {
            if (link == null) connect()
            awaitCancellation()
        } finally {
            watchers--
            if (watchers == 0) close()
        }
    }

    internal fun close() {
        link?.close()
        link = null
        view.update { it.copy(linked = false) }
    }

    private suspend fun connect() {
        val next = store.link(session, KEY)
        link = next
        next.onFrame = ::receive
        next.onNews = news@{ news ->
            if (link !== next) return@news
            when (news) {
                DocumentLink.News.Open -> awaitsSnapshot = true

                is DocumentLink.News.Lost -> view.update { it.copy(linked = false, failure = news.reason) }

                DocumentLink.News.Closed -> {
                    link = null
                    next.close()
                    view.update { it.copy(linked = false) }
                }
            }
        }
        try {
            next.open()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Exception) {
            if (link === next) {
                link = null
                next.close()
            }
            view.update { it.copy(failure = failure.message) }
        }
    }

    // Submitting text while a prompt is pending denies it with the text as feedback instead.
    fun submit(text: String): Boolean {
        val words = text.trim()
        val current = view.value
        if (words.isEmpty() || !current.canWrite) return false
        val question = current.question
        if (question != null) answer(question, ChatAnswer.Deny(words)) else request(ChatRequest.Send(words))
        return true
    }

    fun answer(ask: ChatAsk, answer: ChatAnswer) {
        val current = view.value
        if (ask.id in current.answers || !current.linked) return
        view.update { it.copy(answers = it.answers + (ask.id to answer)) }
        request(ChatRequest.Answer(ask.id, answer))
    }

    /** Hands this agent the work of [peer]'s agent, which gets no message. */
    fun handOff(peer: ChatPeer): Boolean {
        if (!view.value.canWrite) return false
        request(ChatRequest.HandOff(peer.terminal))
        return true
    }

    fun unqueue(queued: ChatQueued) = request(ChatRequest.Unqueue(queued.id))

    fun stop() = request(ChatRequest.Stop)

    /** Asks the agent for its own settings, which arrive in the view's `settings`. */
    fun askSettings() = request(ChatRequest.Settings)

    /**
     * Changes one of the agent's settings to [value], in the words of [ChatSetting.value]. The
     * change shows at once; the host's next update keeps it or puts the setting back.
     */
    fun set(setting: ChatSetting, value: String) {
        view.update { current ->
            val chat = current.chat ?: return@update current
            current.copy(
                chat = chat.copy(
                    settings = chat.settings.map {
                        if (it.key == setting.key) it.copy(value = value, refusal = null) else it
                    }
                )
            )
        }
        request(ChatRequest.Set(setting.key, value))
    }

    private fun request(request: ChatRequest) {
        link?.send(request.json().toString().encodeToByteArray())
    }

    internal fun receive(bytes: ByteArray) {
        val frame = try {
            val read = RuntimeJson.decodeFromString(ChatFrame.serializer(), bytes.decodeToString())
            require((read.snapshot == null) != (read.update == null)) { "a chat frame without one snapshot or update" }
            read
        } catch (failure: IllegalArgumentException) {
            unreadable(failure.message.orEmpty())
            if (!awaitsSnapshot) renew()
            return
        }
        // Clear local answers on a new snapshot: answered asks are already gone on the host, while
        // any ask still present lost its reply during disconnect and must be answerable again.
        frame.snapshot?.let { snapshot ->
            awaitsSnapshot = false
            view.update { it.copy(chat = snapshot, answers = emptyMap(), linked = true, failure = null) }
        }
        frame.update?.let { update ->
            val chat = view.value.chat
            if (awaitsSnapshot || chat == null || update.revision <= chat.revision) return
            if (update.revision != chat.revision + 1) {
                renew()
                return
            }
            val next = chat.applying(update)
            view.update { it.copy(chat = next, answers = it.answers.asked(next)) }
        }
    }

    private fun renew() {
        val current = link ?: return
        awaitsSnapshot = true
        view.update { it.copy(linked = false) }
        current.renewAfterLoss()
    }

    private fun Map<String, ChatAnswer>.asked(chat: ChatSnapshot): Map<String, ChatAnswer> =
        if (isEmpty()) this else filterKeys { id -> chat.asks.any { it.id == id } }

    private companion object {
        const val KEY = "chat"
    }
}

internal fun ChatSnapshot.applying(update: ChatUpdate): ChatSnapshot {
    val items = items.toMutableList()
    var asks = asks
    var queued = queued
    var tasks = tasks
    var commands = commands
    var settings = settings
    var peers = peers
    var facts = facts
    var state = state
    var now = now
    for (change in update.changes) {
        when (change) {
            is ChatChange.Item -> {
                val index = items.indexOfLast { it.id == change.item.id }
                if (index >= 0) items[index] = change.item else items += change.item
            }

            is ChatChange.Text -> {
                val index = items.indexOfLast { it.id == change.id }
                val body = items.getOrNull(index)?.body as? ChatBody.Text ?: continue
                items[index] = items[index].copy(body = body.copy(text = body.text + change.text))
            }

            is ChatChange.Remove -> items.removeAll { it.id == change.id }

            is ChatChange.Asks -> asks = change.asks

            is ChatChange.Queued -> queued = change.queued

            is ChatChange.Tasks -> tasks = change.tasks

            is ChatChange.Commands -> commands = change.commands

            is ChatChange.Settings -> settings = change.settings

            is ChatChange.Peers -> peers = change.peers

            is ChatChange.Facts -> facts = change.facts

            is ChatChange.State -> {
                state = change.state
                now = change.now
            }

            ChatChange.Reset -> {
                items.clear()
                asks = emptyList()
                tasks = emptyList()
            }
        }
    }
    return copy(
        revision = update.revision,
        state = state,
        now = now,
        facts = facts,
        items = items,
        asks = asks,
        queued = queued,
        tasks = tasks,
        commands = commands,
        settings = settings,
        peers = peers
    )
}

internal sealed interface ChatRequest {
    data class Send(val text: String) : ChatRequest

    data class HandOff(val terminal: String) : ChatRequest

    data class Unqueue(val id: String) : ChatRequest

    data class Answer(val ask: String, val answer: ChatAnswer) : ChatRequest

    data object Stop : ChatRequest

    data object Settings : ChatRequest

    data class Set(val key: String, val value: String) : ChatRequest

    fun json(): JsonObject = when (this) {
        is Send -> fields("do" to "send", "text" to text)
        is HandOff -> fields("do" to "handOff", "terminal" to terminal)
        is Unqueue -> fields("do" to "unqueue", "id" to id)
        is Answer -> fields("do" to "answer", "ask" to ask, "answer" to answer.json())
        Stop -> fields("do" to "stop")
        Settings -> fields("do" to "settings")
        is Set -> fields("do" to "set", "key" to key, "value" to value)
    }
}

sealed interface ChatAnswer {
    data object Allow : ChatAnswer

    data object Always : ChatAnswer

    // Non-null text redirects the agent with feedback; null denies and stops the agent's turn.
    data class Deny(val text: String?) : ChatAnswer

    data class Answers(val answers: List<List<String>>) : ChatAnswer

    data class Plan(val choice: PlanChoice) : ChatAnswer

    fun json(): JsonObject = when (this) {
        Allow -> fields("kind" to "allow")

        Always -> fields("kind" to "always")

        is Deny -> if (text == null) fields("kind" to "deny") else fields("kind" to "deny", "text" to text)

        is Answers -> fields("kind" to "answers", "answers" to answers)

        is Plan -> fields(
            "kind" to "plan",
            "choice" to RuntimeJson.encodeToJsonElement(PlanChoice.serializer(), choice)
        )
    }
}
