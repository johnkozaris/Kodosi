package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.InvalidEvent
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.UuidV7
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive

data class TerminalsState(
    val sessions: List<RuntimeSession> = emptyList(),
    val loaded: Boolean = false,
    val listed: Boolean = false
)

/** The terminals that this device can open, as the runtime lists them. */
class TerminalStore(private val commands: CommandSink) {
    private val mutableState = MutableStateFlow(TerminalsState())
    val state: StateFlow<TerminalsState> = mutableState
    private var listing: String? = null
    private var watching: String? = null

    fun session(id: String): RuntimeSession? = mutableState.value.sessions.firstOrNull { it.id == id }

    fun reset() {
        mutableState.value = TerminalsState()
        listing = null
        watching = null
    }

    fun receive(event: RuntimeEvent) {
        when (event.type) {
            "sessions.snapshot" -> receiveSnapshot(event)
            "session.result" -> settleList(event)
        }
    }

    /** Asks for the terminals. Until the answer comes, the list is not known to be empty. */
    fun list() {
        val request = UuidV7.generate()
        listing = request
        commands.perform("session.list", mapOf("requestId" to JsonPrimitive(request)))
    }

    fun absorb(event: RuntimeEvent) = settleList(event)

    /** Tells the runtime which terminal this device shows, so that it streams only that one. */
    fun watch(sessionId: String, shown: Boolean) {
        val next = if (shown) sessionId else watching.takeIf { it != sessionId }
        if (next == watching) return
        watching = next
        val visible = next?.let { listOf(JsonPrimitive(it)) }.orEmpty()
        commands.perform(
            "session.visible",
            mapOf(
                "sessionIds" to JsonArray(visible),
                "watching" to JsonPrimitive(next != null)
            )
        )
    }

    suspend fun open(sessionId: String) {
        commands.request("session.openRemote", mapOf("sessionId" to JsonPrimitive(sessionId)))
    }

    fun disconnect(sessionId: String) {
        commands.perform("session.disconnect", mapOf("sessionId" to JsonPrimitive(sessionId)))
    }

    private fun receiveSnapshot(event: RuntimeEvent) {
        val sessions = event["sessions"] ?: throw InvalidEvent("a terminal list has no terminals")
        val received = RuntimeJson.decodeFromJsonElement(ListSerializer(RuntimeSession.serializer()), sessions)
        if (received.distinctBy { it.id }.size !=
            received.size
        ) {
            throw InvalidEvent("a terminal list repeats a terminal")
        }
        mutableState.update { it.copy(sessions = received, loaded = true) }
    }

    private fun settleList(event: RuntimeEvent) {
        if (listing == null || event.string("requestId") != listing) return
        listing = null
        mutableState.update { it.copy(listed = true) }
    }
}
