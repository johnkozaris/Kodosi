package com.kodosi.mobile.runtime

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

open class RuntimeFailure(message: String) : Exception(message)

/** The answer went with the events that the app dropped when it fell behind. */
class LostAnswer(message: String) : RuntimeFailure(message)

class CommandSink(private val words: Words) {
    enum class Delivery { CALLER, APP }

    private class Pending(val operation: String, val answer: CompletableDeferred<RuntimeEvent>)

    private data class Abandoned(val requestId: String, val operation: String)

    var transport: ((ByteArray) -> Int?)? = null
    var onFailure: (String) -> Unit = {}
    var accountUserId: String? = null
        private set
    var accountEpoch: Long = 0
        private set
    private val pending = HashMap<String, Pending>()

    /** The requests that the app gave up, so that their late answers reach nobody. */
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
        if (pending.size >= MAX_PENDING) throw RuntimeFailure(words.rejected(Ffi.BUSY))
        val requestId = (fields["requestId"] as? JsonPrimitive)?.content ?: UuidV7.generate()
        val answer = CompletableDeferred<RuntimeEvent>()
        pending[requestId] = Pending(type, answer)
        try {
            val result = send(type, fields + ("requestId" to JsonPrimitive(requestId)))
            if (result != Ffi.OK) throw failure(result)
            return withTimeoutOrNull(60_000) { answer.await() } ?: throw RuntimeFailure(words.unavailable)
        } finally {
            pending.remove(requestId)
        }
    }

    fun receive(event: RuntimeEvent): Delivery {
        val requestId = event.string("requestId") ?: return Delivery.APP
        val operation = event.string("operation") ?: return Delivery.APP
        val area = operation.substringBefore('.')
        val failed = event.type == "$area.error"
        if (!failed && event.type != "$area.result") return Delivery.APP
        val waiting = pending[requestId]?.takeIf { it.operation == operation }
            ?: return if (abandoned.remove(Abandoned(requestId, operation))) Delivery.CALLER else Delivery.APP
        pending.remove(requestId)
        if (failed) {
            waiting.answer.completeExceptionally(RuntimeFailure(event.string("message") ?: words.didNotWork))
        } else {
            waiting.answer.complete(event)
        }
        return Delivery.CALLER
    }

    fun cancelPending(error: Throwable = RuntimeFailure(words.unavailable)) {
        val waiting = pending.values.toList()
        pending.clear()
        for (request in waiting) request.answer.completeExceptionally(error)
    }

    /**
     * The answers that the app dropped when it fell behind are lost, so each caller that waits
     * hears that Kodosi cannot tell whether its request worked, and a late answer reaches nobody.
     */
    fun abandonPending() {
        val waiting = pending.toList()
        pending.clear()
        for ((requestId, request) in waiting) {
            abandoned.addLast(Abandoned(requestId, request.operation))
            if (abandoned.size > MAX_PENDING) abandoned.removeFirst()
            request.answer.completeExceptionally(LostAnswer(words.outcomeUnknown))
        }
    }

    private fun failure(result: Int?) = RuntimeFailure(result?.let(words::rejected) ?: words.unavailable)

    private companion object {
        const val MAX_PENDING = 64
    }
}
