package com.kodosi.mobile.people

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.strings
import com.kodosi.mobile.runtime.value
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.Serializable

@Serializable
data class Person(
    val userId: String,
    val handle: String,
    val displayName: String,
    val verified: Boolean = false,
    val identityState: String = "",
    val inviteNeeded: Boolean = false
) {
    val name: String get() = displayName.ifBlank { handle }
    val trusted: Boolean get() = verified && identityState != "changed"
}

data class Invite(val text: String, val code: List<String>)

data class PeopleState(
    val friends: List<Person> = emptyList(),
    val incoming: List<Person> = emptyList(),
    val outgoing: List<Person> = emptyList(),
    val blocked: List<Person> = emptyList(),
    val loaded: Boolean = false
)

class PeopleStore(private val commands: CommandSink) {
    private val mutableState = MutableStateFlow(PeopleState())
    val state: StateFlow<PeopleState> = mutableState

    fun reset() {
        mutableState.value = PeopleState()
    }

    fun receive(event: RuntimeEvent) {
        if (event.type != "friends.snapshot") return
        mutableState.value = PeopleState(
            friends = event.value("friends"),
            incoming = event.value("incoming"),
            outgoing = event.value("outgoing"),
            blocked = event.value("blocked"),
            loaded = true
        )
    }

    fun refresh() = commands.perform("friends.refresh")

    suspend fun invite(): Invite? {
        val answer = commands.request("friends.invite")
        return answer.string("text")?.let { Invite(it, answer.payload.strings("qr")) }
    }

    suspend fun act(operation: String, username: String, invite: String? = null) {
        val fields = fields("username" to username.trim())
        commands.request(operation, if (invite == null) fields else fields + fields("invite" to invite.trim()))
    }

    companion object {
        private val HANDLE = Regex("[\\p{L}\\p{N}._-]{1,64}")
        private val IDENTITY = Regex("[0-9a-fA-F]{64}")

        /** A link or a scanned code starts the add flow only with an invite in the form that the
         *  runtime makes. */
        fun invite(text: String): String? {
            val trimmed = text.trim()
            val parts = trimmed.split(':')
            val valid = parts.size == 3 && parts[0] == "kodosi" &&
                HANDLE.matches(parts[1]) && IDENTITY.matches(parts[2])
            return trimmed.takeIf { valid }
        }

        fun handle(text: String): String = invite(text)?.split(':')?.get(1) ?: text.trim()
    }
}
