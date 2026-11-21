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
    val tasks: List<ChatTask> = emptyList()
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

    /** The agent's list of its work, whole. */
    @Serializable
    @SerialName("tasks")
    data class Tasks(val tasks: List<ChatTask>) : ChatChange

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
    /** No agent of the terminal reports to the chat. */
    @SerialName("away")
    AWAY,

    @SerialName("idle")
    IDLE,

    @SerialName("working")
    WORKING,

    /** The agent waits for a person's answer. */
    @SerialName("asking")
    ASKING
}

/** What the agent does at this moment. */
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

/** [at] is in milliseconds since 1970 on the terminal's computer. */
@Serializable
data class ChatItem(val id: String, val at: Long, val body: ChatBody)

@Serializable
@JsonClassDiscriminator("kind")
sealed interface ChatBody {
    /** A message without a sender was typed in the terminal. */
    @Serializable
    @SerialName("person")
    data class Person(val text: String, val by: ChatPerson? = null) : ChatBody

    @Serializable
    @SerialName("text")
    data class Text(val text: String, val live: Boolean = false) : ChatBody

    @Serializable
    @SerialName("tool")
    data class Tool(val tool: ChatTool) : ChatBody

    /** The agent's turn ended here. */
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
    /** The agent made its conversation shorter to free its context. */
    @Serializable
    @SerialName("compacted")
    data object Compacted : ChatNote

    /** The agent could not continue. [error] is the agent's word for the cause. */
    @Serializable
    @SerialName("failed")
    data class Failed(val error: String, val text: String = "") : ChatNote
}

/** [name] is the agent's name of the tool, [target] the short subject: a command, a file, a pattern, an address. */
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

    /** A helper: another agent that the agent starts for a part of its work. */
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
    /** The agent writes the call. */
    @SerialName("preparing")
    PREPARING,

    /** The call waits for a person's permission or answer. */
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
    /** [background]: the command runs on after its call ended. */
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

    /** For each question in order, what the person chose or wrote. */
    @Serializable
    @SerialName("question")
    data class Question(val questions: List<ChatQuestion>, val answers: List<String> = emptyList()) :
        ChatToolDetail

    @Serializable
    @SerialName("plan")
    data class Plan(val text: String) : ChatToolDetail

    /** A helper of the agent: its kind's name, its steps so far, its last step and the start of what it wrote last. */
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

/**
 * [refused]: the agent did not take the message, in the agent's words. The message waits until a
 * person sends another, removes it, or writes to the agent in the terminal.
 */
@Serializable
data class ChatQueued(val id: String, val text: String, val by: ChatPerson? = null, val refused: String? = null)

/** A person of the terminal: the account, or for a person with no account the view that sent. */
@Serializable
data class ChatPerson(val id: String, val name: String)

/** A step of the list that the agent keeps of its work. */
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
 * [agent] is the agent's name as the terminal's program has it, [cost] what the agent estimates
 * that the conversation cost in US dollars, [helpers] the helpers that work now, [background] the
 * commands that run on after their calls ended, and [models] the models that the agent offers.
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
    val helpers: Int = 0,
    val background: Int = 0,
    val models: List<String> = emptyList()
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
    val headline: Boolean = false
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

/**
 * A question of the agent that is open. The agent shows the same question in its terminal, and
 * the first answer counts. [item] is the item of the tool call that waits.
 */
@Serializable
data class ChatAsk(val id: String, val item: String? = null, val body: AskBody)

@Serializable
@JsonClassDiscriminator("kind")
sealed interface AskBody {
    /** [always] is what the agent offers to allow from now on, and [helper] the task of the helper that asks. */
    @Serializable
    @SerialName("permission")
    data class Permission(val tool: ChatTool, val always: AskAlways? = null, val helper: String = "") : AskBody

    @Serializable
    @SerialName("question")
    data class Question(val questions: List<ChatQuestion>) : AskBody

    /** With the ways on that the agent offers, the one that it recommends first. */
    @Serializable
    @SerialName("plan")
    data class Plan(val text: String, val choices: List<PlanChoice>) : AskBody
}

/** A way on after a plan of the agent. */
@Serializable
enum class PlanChoice {
    /** Carry out the plan, and the agent asks as it is set. */
    @SerialName("go")
    GO,

    /** Carry out the plan, and the agent asks before each change. */
    @SerialName("ask")
    ASK,

    /** Carry out the plan, and the agent changes files without asking. */
    @SerialName("edits")
    EDITS,

    /** Carry out the plan, and the agent's safety check decides what it may do. */
    @SerialName("auto")
    AUTO,

    /** The agent carries out the plan alone and asks nothing. */
    @SerialName("alone")
    ALONE,

    /** Helpers of the agent carry out parts of the plan side by side, alone. */
    @SerialName("helpers")
    HELPERS,

    /** The agent leaves plan mode and does not carry out the plan. */
    @SerialName("leave")
    LEAVE
}

@Serializable
@JsonClassDiscriminator("kind")
sealed interface AskAlways {
    /** Calls that match this rule, such as a command. */
    @Serializable
    @SerialName("rule")
    data class Rule(val tool: String, val text: String) : AskAlways

    /** Each call that this permission mode allows, until the session ends. */
    @Serializable
    @SerialName("mode")
    data class Mode(val mode: ChatMode) : AskAlways

    @Serializable
    @SerialName("folder")
    data class Folder(val path: String) : AskAlways
}

/** When the agent asks before it acts. */
@Serializable
enum class ChatMode {
    /** Before each change and each command. */
    @SerialName("ask")
    ASK,

    /** Before each command, and not before a change of a file. */
    @SerialName("edits")
    EDITS,

    /** The agent plans and changes nothing. */
    @SerialName("plan")
    PLAN,

    /** The agent decides which calls are safe. */
    @SerialName("auto")
    AUTO,

    /** The agent does what a rule allows and refuses the rest. It asks nothing. */
    @SerialName("rules")
    RULES,

    @SerialName("unrestricted")
    UNRESTRICTED
}
