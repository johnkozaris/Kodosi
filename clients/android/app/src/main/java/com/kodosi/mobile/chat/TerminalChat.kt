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

/**
 * The chat of one terminal as this device holds it. [answers] are the answers that a person gave
 * on this device, until their questions are gone.
 */
data class ChatView(
    val chat: ChatSnapshot? = null,
    /** The pipe is open and the chat as it is has come. */
    val linked: Boolean = false,
    val failure: String? = null,
    val answers: Map<String, ChatAnswer> = emptyMap()
) {
    val state: ChatState get() = chat?.state ?: ChatState.AWAY
    val items: List<ChatItem> get() = chat?.items.orEmpty()
    val asks: List<ChatAsk> get() = chat?.asks.orEmpty()
    val queued: List<ChatQueued> get() = chat?.queued.orEmpty()
    val tasks: List<ChatTask> get() = chat?.tasks.orEmpty()
    val facts: ChatFacts get() = chat?.facts ?: ChatFacts()
    val now: ChatNow? get() = chat?.now

    /** The first question that no person on this device answered yet. */
    val question: ChatAsk? get() = asks.firstOrNull { it.id !in answers }
    val working: Boolean get() = state == ChatState.WORKING || state == ChatState.ASKING
    val canWrite: Boolean get() = linked && state != ChatState.AWAY

    fun item(id: String): ChatItem? = chat?.items?.lastOrNull { it.id == id }
}

/** The chat of one terminal, as the terminal's computer keeps it. The pipe is open only while a view watches. */
class TerminalChat internal constructor(
    private val store: CanvasStore,
    val session: String,
    private val unreadable: (String) -> Unit
) {
    val view: StateFlow<ChatView>
        field = MutableStateFlow(ChatView())

    /** What a person writes here stays while the terminal does. */
    var draft = ""
    private var link: DocumentLink? = null
    private var watchers = 0

    /** After each opening of the pipe the host sends the whole chat first, and the changes count from it. */
    private var awaitsSnapshot = true

    /** Each view that shows the chat watches until it ends, and they share one pipe. */
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

    /** A typed message while a question waits answers it: the agent reads the words in place of what it asked to do. */
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

    fun unqueue(queued: ChatQueued) = request(ChatRequest.Unqueue(queued.id))

    fun stop() = request(ChatRequest.Stop)

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
        frame.snapshot?.let { snapshot ->
            awaitsSnapshot = false
            view.update {
                it.copy(chat = snapshot, answers = it.answers.asked(snapshot), linked = true, failure = null)
            }
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

    /**
     * A change that went missing leaves the chat wrong, so the pipe starts again and the host sends
     * the chat as it is.
     */
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
        tasks = tasks
    )
}

/** What the app sends on the chat pipe. */
internal sealed interface ChatRequest {
    /** A message for the agent. It waits in the queue while the agent works. */
    data class Send(val text: String) : ChatRequest

    data class Unqueue(val id: String) : ChatRequest

    data class Answer(val ask: String, val answer: ChatAnswer) : ChatRequest

    /** Stop the agent's turn and the commands of the turn. */
    data object Stop : ChatRequest

    fun json(): JsonObject = when (this) {
        is Send -> fields("do" to "send", "text" to text)
        is Unqueue -> fields("do" to "unqueue", "id" to id)
        is Answer -> fields("do" to "answer", "ask" to ask, "answer" to answer.json())
        Stop -> fields("do" to "stop")
    }
}

sealed interface ChatAnswer {
    data object Allow : ChatAnswer

    /** Allow, with what the question offers to allow from now on. */
    data object Always : ChatAnswer

    /** With words, the agent reads them and continues. Without, the agent stops. */
    data class Deny(val text: String?) : ChatAnswer

    /** For each question in order, the labels that the person chose or one text of their own. */
    data class Answers(val answers: List<List<String>>) : ChatAnswer

    /** The way on after a plan that the person chose. */
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
