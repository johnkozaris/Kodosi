package com.kodosi.mobile.runtime

import kotlinx.coroutines.CompletableDeferred
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

class RuntimeFailure(message: String) : Exception(message)

/** Sends commands to the runtime for the current account and pairs each request with its answer. */
class CommandSink(private val words: Words) {
    enum class Delivery { CALLER, APP }

    private class Pending(val operation: String, val answer: CompletableDeferred<RuntimeEvent>)

    var transport: ((ByteArray) -> Int?)? = null
    var onFailure: (String) -> Unit = {}
    var accountUserId: String? = null
        private set
    var accountEpoch: Long = 0
        private set
    private val pending = HashMap<String, Pending>()

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
            return answer.await()
        } finally {
            pending.remove(requestId)
        }
    }

    fun receive(event: RuntimeEvent): Delivery {
        val requestId = event.string("requestId") ?: return Delivery.APP
        val waiting = pending[requestId] ?: return Delivery.APP
        if (event.string("operation") != waiting.operation) return Delivery.APP
        val area = waiting.operation.substringBefore('.')
        when (event.type) {
            "$area.error" -> waiting.answer.completeExceptionally(
                RuntimeFailure(event.string("message") ?: words.didNotWork)
            )

            "$area.result" -> waiting.answer.complete(event)

            else -> return Delivery.APP
        }
        pending.remove(requestId)
        return Delivery.CALLER
    }

    fun cancelPending(error: Throwable = RuntimeFailure(words.unavailable)) {
        val waiting = pending.values.toList()
        pending.clear()
        for (request in waiting) request.answer.completeExceptionally(error)
    }

    private fun failure(result: Int?) = RuntimeFailure(result?.let(words::rejected) ?: words.unavailable)

    private companion object {
        const val MAX_PENDING = 64
    }
}
