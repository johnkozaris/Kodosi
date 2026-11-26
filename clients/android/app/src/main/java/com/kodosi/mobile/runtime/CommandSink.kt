package com.kodosi.mobile.runtime

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

open class RuntimeFailure(message: String) : Exception(message)

// Thrown when event overflow drops a pending command's reply.
class LostAnswer(message: String) : RuntimeFailure(message)

// Thrown when the owner refused the request in the app, or its question expired. It ends without a message.
class Refused(message: String) : RuntimeFailure(message)

// Called from both the main thread and the dedicated browser executor thread.
class CommandSink(private val words: Words) {
    enum class Delivery { CALLER, APP }

    private class Pending(val operation: String, val answer: CompletableDeferred<RuntimeEvent>)

    private data class Abandoned(val requestId: String, val operation: String)

    @Volatile
    var transport: ((ByteArray) -> Int?)? = null
    var onFailure: (String) -> Unit = {}

    @Volatile
    var accountUserId: String? = null
        private set

    @Volatile
    var accountEpoch: Long = 0
        private set
    private val pending = HashMap<String, Pending>()

    // Tracks abandoned request IDs so late replies are swallowed instead of surfacing to the app.
    private val abandoned = ArrayDeque<Abandoned>()

    fun setAccount(userId: String?, epoch: Long) {
        if (epoch == accountEpoch && userId == accountUserId) return
        cancelPending(RuntimeFailure(words.accountChanged))
        accountUserId = userId
        accountEpoch = epoch
    }

    fun send(type: String, fields: Map<String, JsonElement> = emptyMap()): Int? {
        val transport = transport ?: return null
        val command = buildJsonObject {
            for ((key, value) in fields) put(key, value)
            if (!fields.containsKey("requestId")) put("requestId", JsonPrimitive(UuidV7.generate()))
            put("type", JsonPrimitive(type))
            put("accountUserId", accountUserId?.let(::JsonPrimitive) ?: JsonNull)
            put("accountEpoch", JsonPrimitive(accountEpoch))
        }
        return transport(command.toString().encodeToByteArray())
    }

    fun perform(type: String, fields: Map<String, JsonElement> = emptyMap()) {
        val result = send(type, fields)
        if (result != Ffi.OK) onFailure(failure(result).message.orEmpty())
    }

    suspend fun request(type: String, fields: Map<String, JsonElement> = emptyMap()): RuntimeEvent {
        val requestId = (fields["requestId"] as? JsonPrimitive)?.content ?: UuidV7.generate()
        val answer = CompletableDeferred<RuntimeEvent>()
        synchronized(pending) {
            if (pending.size >= MAX_PENDING) throw RuntimeFailure(words.rejected(Ffi.BUSY))
            pending[requestId] = Pending(type, answer)
        }
        try {
            val result = send(type, fields + ("requestId" to JsonPrimitive(requestId)))
            if (result != Ffi.OK) throw failure(result)
            val limit = if (type in MAY_ASK_OWNER) OWNER_ANSWER_TIME + ANSWER_TIME else ANSWER_TIME
            return withTimeoutOrNull(limit) { answer.await() } ?: throw RuntimeFailure(words.unavailable)
        } finally {
            synchronized(pending) { pending.remove(requestId) }
        }
    }

    fun receive(event: RuntimeEvent): Delivery {
        val requestId = event.string("requestId") ?: return Delivery.APP
        val operation = event.string("operation") ?: return Delivery.APP
        val area = operation.substringBefore('.')
        val failed = event.type == "$area.error"
        if (!failed && event.type != "$area.result") return Delivery.APP
        val waiting = synchronized(pending) {
            pending[requestId]?.takeIf { it.operation == operation }?.also { pending.remove(requestId) }
                ?: return if (abandoned.remove(Abandoned(requestId, operation))) Delivery.CALLER else Delivery.APP
        }
        if (failed) {
            val message = event.string("message") ?: words.didNotWork
            waiting.answer.completeExceptionally(if (event.refused) Refused(message) else RuntimeFailure(message))
        } else {
            waiting.answer.complete(event)
        }
        return Delivery.CALLER
    }

    fun cancelPending(error: Throwable = RuntimeFailure(words.unavailable)) {
        val waiting = synchronized(pending) { pending.values.toList().also { pending.clear() } }
        for (request in waiting) request.answer.completeExceptionally(error)
    }

    fun abandonPending() {
        val waiting = synchronized(pending) { pending.toList().also { pending.clear() } }
        for ((requestId, request) in waiting) {
            abandoned.addLast(Abandoned(requestId, request.operation))
            if (abandoned.size > MAX_PENDING) abandoned.removeFirst()
            request.answer.completeExceptionally(LostAnswer(words.outcomeUnknown))
        }
    }

    private fun failure(result: Int?) = RuntimeFailure(result?.let(words::rejected) ?: words.unavailable)

    private companion object {
        const val MAX_PENDING = 64
        const val ANSWER_TIME = 60_000L

        // The runtime may hold these until the owner answers its question in the app. It ends an
        // unanswered question after five minutes with its own error, which should arrive first.
        const val OWNER_ANSWER_TIME = 5 * 60_000L
        val MAY_ASK_OWNER = setOf(
            "session.share",
            "session.attachProject",
            "session.create",
            "project.invite",
            "friends.request.send",
            "friends.request.accept",
            "friends.identity.trust",
            "friends.verify"
        )
    }
}
