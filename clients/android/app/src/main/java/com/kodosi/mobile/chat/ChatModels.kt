@file:OptIn(ExperimentalSerializationApi::class)

package com.kodosi.mobile.chat

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonClassDiscriminator

@Serializable
internal data class ChatFrame(val snapshot: ChatSnapshot? = null, val update: ChatUpdate? = null)

@Serializable
data class ChatSnapshot(
    val revision: Long,
    val state: ChatState,
    val now: ChatNow? = null,
    val facts: ChatFacts,
    val items: List<ChatItem>,
    val asks: List<ChatAsk> = emptyList(),
    val queued: List<ChatQueued> = emptyList(),
    val tasks: List<ChatTask> = emptyList(),
    val commands: List<ChatCommand> = emptyList()
)

@Serializable
data class ChatUpdate(val revision: Long, val changes: List<ChatChange>)

@Serializable
@JsonClassDiscriminator("change")
sealed interface ChatChange {
    @Serializable
    @SerialName("item")
    data class Item(val item: ChatItem) : ChatChange

    @Serializable
    @SerialName("text")
    data class Text(val id: String, val text: String) : ChatChange

    @Serializable
    @SerialName("remove")
    data class Remove(val id: String) : ChatChange

    @Serializable
    @SerialName("asks")
    data class Asks(val asks: List<ChatAsk>) : ChatChange

    @Serializable
    @SerialName("queued")
    data class Queued(val queued: List<ChatQueued>) : ChatChange

    @Serializable
    @SerialName("tasks")
    data class Tasks(val tasks: List<ChatTask>) : ChatChange

    /** The agent's slash commands, whole. */
    @Serializable
    @SerialName("commands")
    data class Commands(val commands: List<ChatCommand>) : ChatChange

    @Serializable
    @SerialName("facts")
    data class Facts(val facts: ChatFacts) : ChatChange

    @Serializable
    @SerialName("state")
    data class State(val state: ChatState, val now: ChatNow? = null) : ChatChange

    @Serializable
    @SerialName("reset")
    data object Reset : ChatChange
}

@Serializable
enum class ChatState {
    // No agent is attached to this terminal's chat.
    @SerialName("away")
    AWAY,

    @SerialName("idle")
    IDLE,

    @SerialName("working")
    WORKING,

    @SerialName("asking")
    ASKING
}

@Serializable
@JsonClassDiscriminator("kind")
sealed interface ChatNow {
    @Serializable
    @SerialName("thinking")
    data object Thinking : ChatNow

    @Serializable
    @SerialName("writing")
    data object Writing : ChatNow

    @Serializable
    @SerialName("tool")
    data class Tool(val item: String) : ChatNow

    @Serializable
    @SerialName("compacting")
    data object Compacting : ChatNow
}

// at is a Unix epoch timestamp in milliseconds from the host computer's clock (0 if unknown).
@Serializable
data class ChatItem(val id: String, val at: Long, val body: ChatBody)

@Serializable
@JsonClassDiscriminator("kind")
sealed interface ChatBody {
    // A null sender means the message was typed directly in the terminal.
    @Serializable
    @SerialName("person")
    data class Person(val text: String, val by: ChatPerson? = null) : ChatBody

    @Serializable
    @SerialName("text")
    data class Text(val text: String, val live: Boolean = false) : ChatBody

    @Serializable
    @SerialName("tool")
    data class Tool(val tool: ChatTool) : ChatBody

    @Serializable
    @SerialName("turn")
    data class Turn(val end: TurnEnd, val ms: Long = 0) : ChatBody

    @Serializable
    @SerialName("note")
    data class Note(val note: ChatNote) : ChatBody
}

@Serializable
enum class TurnEnd {
    @SerialName("done")
    DONE,

    @SerialName("stopped")
    STOPPED,

    @SerialName("failed")
    FAILED,

    @SerialName("refused")
    REFUSED
}

@Serializable
@JsonClassDiscriminator("kind")
sealed interface ChatNote {
    @Serializable
    @SerialName("compacted")
    data object Compacted : ChatNote

    @Serializable
    @SerialName("signedOut")
    data object SignedOut : ChatNote

    /** The agent's summary of what happened while the person was away. */
    @Serializable
    @SerialName("recap")
    data class Recap(val text: String) : ChatNote

    @Serializable
    @SerialName("failed")
    data class Failed(val error: String, val text: String = "") : ChatNote
}

@Serializable
data class ChatTool(
    val name: String,
    val kind: ChatToolKind,
    val state: ChatToolState,
    val target: String = "",
    val detail: ChatToolDetail? = null,
    val ms: Long? = null
)

@Serializable
enum class ChatToolKind {
    @SerialName("command")
    COMMAND,

    @SerialName("read")
    READ,

    @SerialName("edit")
    EDIT,

    @SerialName("write")
    WRITE,

    @SerialName("search")
    SEARCH,

    @SerialName("web")
    WEB,

    @SerialName("agent")
    AGENT,

    @SerialName("skill")
    SKILL,

    @SerialName("question")
    QUESTION,

    @SerialName("plan")
    PLAN,

    @SerialName("other")
    OTHER
}

@Serializable
enum class ChatToolState {
    @SerialName("preparing")
    PREPARING,

    @SerialName("asking")
    ASKING,

    @SerialName("running")
    RUNNING,

    @SerialName("done")
    DONE,

    @SerialName("failed")
    FAILED,

    @SerialName("denied")
    DENIED,

    @SerialName("stopped")
    STOPPED;

    val isOpen: Boolean get() = this == PREPARING || this == ASKING || this == RUNNING
}

@Serializable
@JsonClassDiscriminator("kind")
sealed interface ChatToolDetail {
    @Serializable
    @SerialName("command")
    data class Command(val command: String, val output: String = "", val background: Boolean = false) :
        ChatToolDetail

    @Serializable
    @SerialName("change")
    data class Change(val path: String, val added: Int, val removed: Int, val lines: List<ChangeLine>) :
        ChatToolDetail

    @Serializable
    @SerialName("output")
    data class Output(val text: String) : ChatToolDetail

    @Serializable
    @SerialName("question")
    data class Question(val questions: List<ChatQuestion>, val answers: List<String> = emptyList()) :
        ChatToolDetail

    @Serializable
    @SerialName("plan")
    data class Plan(val text: String) : ChatToolDetail

    @Serializable
    @SerialName("agent")
    data class Agent(val name: String = "", val steps: Int = 0, val doing: ChatStep? = null, val said: String = "") :
        ChatToolDetail
}

@Serializable
data class ChatStep(val kind: ChatToolKind, val target: String = "")

@Serializable
data class ChangeLine(val sign: ChangeSign, val text: String)

@Serializable
enum class ChangeSign {
    @SerialName("same")
    SAME,

    @SerialName("added")
    ADDED,

    @SerialName("removed")
    REMOVED
}

// If refused is non-null, the message stays queued until removed or superseded by new input.
@Serializable
data class ChatQueued(
    val id: String,
    val text: String,
    val by: ChatPerson? = null,
    val refused: String? = null,
    /** The agent shows this command in its terminal, where a person finishes it. */
    val terminal: Boolean = false,
    /**
     * The agent has this message for the turn that runs and reads it before its next step, so it
     * can no longer be removed.
     */
    val given: Boolean = false
)

/** A slash command of the agent, as its own menu lists it. */
@Serializable
data class ChatCommand(
    val name: String,
    val description: String = "",
    /** The control of the chat that does what the command does, in place of the agent's own view in its terminal. */
    val opens: ChatControl? = null
)

@Serializable
enum class ChatControl {
    /** The agent's model, effort and fast mode. */
    @SerialName("model")
    MODEL,

    /** The limits of the agent's plan, its context and the cost of the conversation. */
    @SerialName("usage")
    USAGE
}

@Serializable
data class ChatPerson(val id: String, val name: String)

@Serializable
data class ChatTask(val text: String, val state: TaskState)

@Serializable
enum class TaskState {
    @SerialName("pending")
    PENDING,

    @SerialName("working")
    WORKING,

    @SerialName("done")
    DONE
}

/**
 * [agent] is the agent's name in the same form as the terminal's program, [cost] the agent's estimate
 * of what the conversation has cost in US dollars, and [background] the number of commands still
 * running after their calls ended.
 */
@Serializable
data class ChatFacts(
    val agent: String? = null,
    val version: String? = null,
    val model: String? = null,
    val effort: String? = null,
    val mode: ChatMode? = null,
    val folder: String? = null,
    val context: ChatContext? = null,
    val limits: List<UsageLimit> = emptyList(),
    val cost: Double? = null,
    val background: Int = 0
)

@Serializable
data class ChatContext(val tokens: Long, val window: Long) {
    val used: Float get() = if (window == 0L) 0f else minOf(1f, tokens.toFloat() / window)
}

@Serializable
data class UsageLimit(
    val hours: Int? = null,
    val scope: String? = null,
    val used: Int,
    val level: UsageLevel,
    val resetsAtMs: Long? = null
) {
    companion object {
        fun nearest(limits: List<UsageLimit>): UsageLimit? =
            limits.maxWithOrNull(compareBy<UsageLimit> { it.level }.thenBy { it.used })
    }
}

@Serializable
enum class UsageLevel {
    @SerialName("normal")
    NORMAL,

    @SerialName("near")
    NEAR,

    @SerialName("reached")
    REACHED
}

@Serializable
data class ChatQuestion(
    val question: String,
    val header: String = "",
    val options: List<ChatOption>,
    val multiple: Boolean = false
)

@Serializable
data class ChatOption(val label: String, val description: String = "")

@Serializable
data class ChatAsk(val id: String, val item: String? = null, val body: AskBody)

@Serializable
@JsonClassDiscriminator("kind")
sealed interface AskBody {
    @Serializable
    @SerialName("permission")
    data class Permission(val tool: ChatTool, val always: AskAlways? = null, val helper: String = "") : AskBody

    @Serializable
    @SerialName("question")
    data class Question(val questions: List<ChatQuestion>) : AskBody

    // choices is ordered with the agent's recommended option first.
    @Serializable
    @SerialName("plan")
    data class Plan(val text: String, val choices: List<PlanChoice>) : AskBody
}

@Serializable
enum class PlanChoice {
    @SerialName("go")
    GO,

    @SerialName("ask")
    ASK,

    @SerialName("edits")
    EDITS,

    @SerialName("auto")
    AUTO,

    @SerialName("alone")
    ALONE,

    @SerialName("helpers")
    HELPERS,

    @SerialName("leave")
    LEAVE
}

@Serializable
@JsonClassDiscriminator("kind")
sealed interface AskAlways {
    @Serializable
    @SerialName("rule")
    data class Rule(val tool: String, val text: String) : AskAlways

    @Serializable
    @SerialName("mode")
    data class Mode(val mode: ChatMode) : AskAlways

    @Serializable
    @SerialName("folder")
    data class Folder(val name: String) : AskAlways
}

@Serializable
enum class ChatMode {
    @SerialName("ask")
    ASK,

    @SerialName("edits")
    EDITS,

    @SerialName("plan")
    PLAN,

    @SerialName("auto")
    AUTO,

    @SerialName("rules")
    RULES,

    @SerialName("unrestricted")
    UNRESTRICTED
}
