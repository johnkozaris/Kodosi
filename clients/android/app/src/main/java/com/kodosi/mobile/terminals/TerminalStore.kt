package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.InvalidEvent
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeFailure
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.fields
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

data class OpenResult(val failure: String?)

class TerminalStore(private val commands: CommandSink) {
    val state: StateFlow<TerminalsState>
        field = MutableStateFlow(TerminalsState())

    val openResults: StateFlow<Map<String, OpenResult>>
        field = MutableStateFlow(emptyMap())

    /** The terminals that show their chat in place of their screen. */
    val chatting: StateFlow<Set<String>>
        field = MutableStateFlow(emptySet())
    private val opening = HashMap<String, Any>()
    private var listing: String? = null
    private var watching: String? = null

    fun session(id: String): RuntimeSession? = state.value.sessions.firstOrNull { it.id == id }

    fun notificationTarget(id: String): RuntimeSession? {
        val listed = state.value.sessions.filterNot { it.hidden }
        return listed.firstOrNull { it.id == id } ?: listed.firstOrNull { it.restoredFrom == id && it.isOwner }
    }

    fun reset() {
        state.value = TerminalsState()
        openResults.value = emptyMap()
        chatting.value = emptySet()
        opening.clear()
        listing = null
        watching = null
    }

    fun receive(event: RuntimeEvent) {
        when (event.type) {
            "sessions.snapshot" -> receiveSnapshot(event)
            "session.result" -> settleList(event)
        }
    }

    /** Until the answer comes, the list is not known to be empty. */
    fun list() {
        val request = UuidV7.generate()
        listing = request
        commands.perform("session.list", mapOf("requestId" to JsonPrimitive(request)))
    }

    fun absorb(event: RuntimeEvent) = settleList(event)

    /** The runtime streams only the terminal that this device shows. */
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

    /**
     * A request that stops waiting still ends the terminal's opening, so that its screen offers Connect instead of
     * waiting for an answer that nobody reads.
     */
    suspend fun open(sessionId: String): OpenResult {
        val attempt = Any()
        opening[sessionId] = attempt
        openResults.update { it - sessionId }
        var result = OpenResult(failure = null)
        try {
            commands.request("session.openRemote", mapOf("sessionId" to JsonPrimitive(sessionId)))
        } catch (failure: RuntimeFailure) {
            result = OpenResult(failure.message.orEmpty())
        } finally {
            if (opening[sessionId] === attempt) {
                opening.remove(sessionId)
                openResults.update { it + (sessionId to result) }
            }
        }
        return result
    }

    fun showChat(shown: Boolean, sessionId: String) {
        chatting.update { if (shown) it + sessionId else it - sessionId }
    }

    fun disconnect(sessionId: String) {
        opening.remove(sessionId)
        openResults.update { it - sessionId }
        commands.perform("session.disconnect", mapOf("sessionId" to JsonPrimitive(sessionId)))
    }

    suspend fun act(
        operation: String,
        session: RuntimeSession,
        extra: Map<String, kotlinx.serialization.json.JsonElement> = emptyMap()
    ) = commands.request(
        operation,
        fields("sessionId" to session.id, "expectedRuntimeIncarnationId" to session.incarnationId) + extra
    )

    private fun receiveSnapshot(event: RuntimeEvent) {
        val sessions = event["sessions"] ?: throw InvalidEvent("a terminal list has no terminals")
        val received = RuntimeJson.decodeFromJsonElement(ListSerializer(RuntimeSession.serializer()), sessions)
        if (received.distinctBy { it.id }.size !=
            received.size
        ) {
            throw InvalidEvent("a terminal list repeats a terminal")
        }
        state.update { it.copy(sessions = received, loaded = true) }
        chatting.update { shown -> shown.filterTo(mutableSetOf()) { id -> received.any { it.id == id } } }
    }

    private fun settleList(event: RuntimeEvent) {
        if (listing == null || event.string("requestId") != listing) return
        listing = null
        state.update { it.copy(listed = true) }
    }
}
