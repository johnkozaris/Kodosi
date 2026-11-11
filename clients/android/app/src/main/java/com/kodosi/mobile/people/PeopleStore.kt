package com.kodosi.mobile.people

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.fields
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

    suspend fun invite(): String? = commands.request("friends.invite").string("text")

    suspend fun act(operation: String, username: String, invite: String? = null) {
        val fields = fields("username" to username.trim())
        commands.request(operation, if (invite == null) fields else fields + fields("invite" to invite.trim()))
    }
}
