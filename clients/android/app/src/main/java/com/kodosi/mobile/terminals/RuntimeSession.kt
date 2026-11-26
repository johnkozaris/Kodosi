package com.kodosi.mobile.terminals

import com.kodosi.mobile.canvas.Canvas
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class ProgramStatus(val state: State, val kind: Kind? = null) {
    @Serializable
    enum class State {
        @SerialName("idle")
        IDLE,

        @SerialName("working")
        WORKING,

        @SerialName("done")
        DONE,

        @SerialName("blocked")
        BLOCKED,

        @SerialName("error")
        ERROR
    }

    @Serializable
    enum class Kind {
        @SerialName("permission")
        PERMISSION,

        @SerialName("question")
        QUESTION,

        @SerialName("auth")
        AUTH
    }
}

/** The lines that a terminal's branch folder added and removed since its branch started. */
@Serializable
data class BranchChanges(val added: Int, val removed: Int)

@Serializable
data class RuntimeSession(
    val id: String,
    val incarnationId: String,
    val kind: Kind,
    val name: String,
    val program: String? = null,
    val programAccount: Int? = null,
    val programStatus: ProgramStatus? = null,
    val programSettings: AgentSettings? = null,
    val programLive: AgentLive? = null,
    val programPending: AgentSettings? = null,
    val workingDir: String? = null,
    val ownerUserId: String? = null,
    val ownerName: String? = null,
    val hostDeviceId: String? = null,
    val hostName: String? = null,
    val isOwner: Boolean = false,
    val status: Status,
    val connectionState: Connection,
    val stalled: Boolean = false,
    val direct: Boolean = false,
    @SerialName("working")
    val isWorking: Boolean,
    @SerialName("waiting")
    val waitState: ProgramStatus.State? = null,
    val activity: String? = null,
    val message: String? = null,
    val projectName: String? = null,
    val attention: Boolean = false,
    val hidden: Boolean = false,
    val projectId: String? = null,
    val sharedWith: List<String> = emptyList(),
    val restoredFrom: String? = null,
    val canvas: Canvas? = null,
    // Preferred face reported by the agent (chat vs terminal), or null if none.
    val face: TerminalFace? = null,
    /** How many helpers of the terminal's agent are working now. */
    val helpers: Int = 0,
    // Terminal color theme chosen by the host computer for all viewers.
    val look: TerminalLook = TerminalLook.DARK,
    /** The agent stopped in the middle of a task when its computer's Kodosi stopped. */
    val stopped: Boolean = false,
    /** What the terminal's branch folder changed since its branch started. */
    val changes: BranchChanges? = null
) {
    fun shownFace(pick: FacePick?): TerminalFace = pick?.face ?: face ?: TerminalFace.TERMINAL

    // Only its owner's devices see a computer's screen.
    val showsScreen: Boolean get() = isOwner && kind == Kind.REMOTE

    fun keeps(pick: FacePick): Boolean = face == null || pick.over == face

    @Serializable
    enum class Kind {
        @SerialName("local")
        LOCAL,

        @SerialName("remote")
        REMOTE
    }

    @Serializable
    enum class Status {
        @SerialName("running")
        RUNNING,

        @SerialName("reconnecting")
        RECONNECTING,

        @SerialName("offline")
        OFFLINE,

        @SerialName("closing")
        CLOSING
    }

    @Serializable
    enum class Connection {
        @SerialName("local")
        LOCAL,

        @SerialName("connecting")
        CONNECTING,

        @SerialName("connected")
        CONNECTED,

        @SerialName("offline")
        OFFLINE,

        @SerialName("blocked")
        BLOCKED
    }

    val isConnected: Boolean get() = connectionState == Connection.LOCAL || connectionState == Connection.CONNECTED
    val canControl: Boolean get() = isConnected && status == Status.RUNNING

    /** The agent stopped in the middle of a task, and anyone here may let it go on. */
    val offersContinue: Boolean get() = stopped && canControl && !isWorking
    val waitsForConnection: Boolean
        get() = kind == Kind.REMOTE && isConnected && (status == Status.RECONNECTING || stalled)
    val canOpen: Boolean get() = status != Status.CLOSING && connectionState != Connection.BLOCKED
    val hostOffline: Boolean get() = kind == Kind.REMOTE && status == Status.OFFLINE
    val isTroubled: Boolean
        get() = connectionState == Connection.BLOCKED || status == Status.OFFLINE ||
            (connectionState == Connection.OFFLINE && message != null)

    fun unreachable(online: Boolean): Boolean = !online && kind == Kind.REMOTE && connectionState != Connection.BLOCKED

    val needsUser: Boolean get() = waitState == ProgramStatus.State.BLOCKED

    val agent: AgentKind get() = AgentKind.of(program)

    val folderName: String? get() = workingDir?.let(::lastName)
}

/** The last component of a path from any computer, or the path itself for a root. */
fun lastName(path: String): String =
    path.trimEnd('/', '\\').substringAfterLast('/').substringAfterLast('\\').ifEmpty { path }

@Serializable
enum class TerminalFace {
    @SerialName("terminal")
    TERMINAL,

    @SerialName("chat")
    CHAT
}

// Local user override of a terminal's face; it holds until the agent reports a face other than
// [over] (e.g. drops automatically if the agent switches back to terminal for auth).
data class FacePick(val face: TerminalFace, val over: TerminalFace?)

@Serializable
enum class TerminalLook {
    @SerialName("dark")
    DARK,

    @SerialName("light")
    LIGHT
}

enum class AgentKind {
    CLAUDE,
    CODEX,
    COPILOT,
    CURSOR,
    OPENCODE,
    ANTIGRAVITY,
    SHELL
    ;

    val isAgent: Boolean get() = this != SHELL

    companion object {
        fun of(program: String?): AgentKind = when (program) {
            "cursor-agent" -> CURSOR
            "agy" -> ANTIGRAVITY
            null -> SHELL
            else -> entries.firstOrNull { it.name.lowercase() == program } ?: SHELL
        }

        fun named(agent: String?): AgentKind {
            val name = agent?.lowercase().orEmpty()
            return entries.firstOrNull { it.isAgent && name.contains(it.name.lowercase()) } ?: SHELL
        }
    }
}
