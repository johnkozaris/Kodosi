package com.kodosi.mobile.terminals

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
    val title: String? = null,
    val programStatus: ProgramStatus? = null,
    val workingDir: String? = null,
    val ownerUserId: String? = null,
    val ownerName: String? = null,
    val hostDeviceId: String? = null,
    val hostName: String? = null,
    val isOwner: Boolean = false,
    val status: Status,
    val connectionState: Connection,
    val message: String? = null,
    val missionName: String? = null,
    val attention: Boolean = false,
    val hidden: Boolean = false
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

    fun unreachable(online: Boolean): Boolean = !online && kind == Kind.REMOTE && connectionState != Connection.BLOCKED

    val isWorking: Boolean
        get() = status != Status.CLOSING &&
            (programStatus?.state?.let { it == ProgramStatus.State.WORKING } ?: (canControl && titleSpins))

    val needsUser: Boolean get() = status != Status.CLOSING && programStatus?.state == ProgramStatus.State.BLOCKED

    val waitState: ProgramStatus.State?
        get() = when (programStatus?.state) {
            ProgramStatus.State.BLOCKED -> if (needsUser) ProgramStatus.State.BLOCKED else null
            ProgramStatus.State.DONE -> ProgramStatus.State.DONE
            ProgramStatus.State.ERROR -> ProgramStatus.State.ERROR
            else -> null
        }

    val activity: String?
        get() = programStatus?.caption ?: titleWords?.takeIf { it != name }

    val agent: AgentKind get() = AgentKind.of(program)

    val folderName: String? get() = workingDir?.trimEnd('/', '\\')?.substringAfterLast('/')?.substringAfterLast('\\')

    private val titleSpins: Boolean get() = title?.firstOrNull()?.let { it in BRAILLE } == true

    private val titleWords: String?
        get() = title?.dropWhile { it in BRAILLE || it in STATUS_GLYPHS || it.isWhitespace() }
            ?.filterNot { it.isISOControl() }
            ?.ifEmpty { null }
}

private val BRAILLE = '\u2800'..'\u28FF'
private val STATUS_GLYPHS = setOf('\u2733', '\u2722', '\u2736', '\u273B', '\u273D', '\u00B7', '\u25CF', '\u25CB')

enum class AgentKind {
    CLAUDE,
    CODEX,
    COPILOT,
    CURSOR,
    GEMINI,
    OPENCODE,
    SHELL
    ;

    companion object {
        fun of(program: String?): AgentKind = when (program) {
            "cursor-agent" -> CURSOR
            null -> SHELL
            else -> entries.firstOrNull { it.name.lowercase() == program } ?: SHELL
        }
    }
}
