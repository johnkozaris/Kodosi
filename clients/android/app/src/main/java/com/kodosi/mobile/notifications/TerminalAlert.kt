package com.kodosi.mobile.notifications

import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.ProgramStatus
import java.util.UUID

data class TerminalAlert(val terminal: String, val agent: AgentKind, val status: ProgramStatus) {
    companion object {
        fun parse(data: Map<String, String>): TerminalAlert? {
            val terminal = data["terminal"]?.takeIf { id ->
                id.length == 36 && runCatching { UUID.fromString(id).toString() == id.lowercase() }.getOrDefault(false)
            } ?: return null
            val state = when (data["state"]) {
                "blocked" -> ProgramStatus.State.BLOCKED
                "done" -> ProgramStatus.State.DONE
                "error" -> ProgramStatus.State.ERROR
                else -> return null
            }
            val kind = when (data["kind"]) {
                "permission" -> ProgramStatus.Kind.PERMISSION
                "question" -> ProgramStatus.Kind.QUESTION
                "auth" -> ProgramStatus.Kind.AUTH
                else -> null
            }
            return TerminalAlert(terminal, AgentKind.of(data["agent"]), ProgramStatus(state, kind))
        }
    }
}
