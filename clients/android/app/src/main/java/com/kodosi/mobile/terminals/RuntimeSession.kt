package com.kodosi.mobile.terminals

import com.kodosi.mobile.canvas.Canvas
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class ProgramStatus(
    val state: State,
    val kind: Kind? = null,
    val title: String? = null,
    val message: String? = null
) {
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

    val caption: String?
        get() = listOfNotNull(title, message).joinToString(": ").ifEmpty { null }
}

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
    @SerialName("working")
    val isWorking: Boolean,
    @SerialName("waiting")
    val waitState: ProgramStatus.State? = null,
    val activity: String? = null,
    val message: String? = null,
    val missionName: String? = null,
    val attention: Boolean = false,
    val hidden: Boolean = false,
    val missionId: String? = null,
    val connectedUsers: List<String>? = null,
    val sharedWith: List<String> = emptyList(),
    val restoredFrom: String? = null,
    val canvas: Canvas? = null
) {
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
    val canOpen: Boolean get() = status != Status.CLOSING && connectionState != Connection.BLOCKED
    val hostOffline: Boolean get() = kind == Kind.REMOTE && status == Status.OFFLINE
    val isTroubled: Boolean
        get() = connectionState == Connection.BLOCKED || status == Status.OFFLINE ||
            (connectionState == Connection.OFFLINE && message != null)

    /** Whether Kodosi cannot reach this terminal of another computer because this device is offline. */
    fun unreachable(online: Boolean): Boolean = !online && kind == Kind.REMOTE && connectionState != Connection.BLOCKED

    val needsUser: Boolean get() = waitState == ProgramStatus.State.BLOCKED

    val agent: AgentKind get() = AgentKind.of(program)

    val folderName: String? get() = workingDir?.trimEnd('/', '\\')?.substringAfterLast('/')?.substringAfterLast('\\')
}

/** The program of a terminal, which picks its mark. */
enum class AgentKind {
    CLAUDE,
    CODEX,
    COPILOT,
    CURSOR,
    GEMINI,
    OPENCODE,
    SHELL
    ;

    val isAgent: Boolean get() = this != SHELL

    companion object {
        fun of(program: String?): AgentKind = when (program) {
            "cursor-agent" -> CURSOR
            null -> SHELL
            else -> entries.firstOrNull { it.name.lowercase() == program } ?: SHELL
        }

        fun named(agent: String?): AgentKind {
            val name = agent?.lowercase().orEmpty()
            return entries.firstOrNull { it.isAgent && name.contains(it.name.lowercase()) } ?: SHELL
        }
    }
}
